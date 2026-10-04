"use client";

/**
 * Alerts (UI.md section 28).
 *
 * Reads the real alert feed and the real summary counts. Acknowledging calls the
 * acknowledge endpoint, so an alert's status reflects an actual action rather
 * than a local toggle.
 */

import Link from "next/link";
import { useState } from "react";
import { AlertTriangle, Bell, CheckCircle2, Filter, Info } from "lucide-react";

import { useList, useQuery } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { formatRelative, formatTimestamp } from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary } from "@/components/states";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "cn";
import { AlertRuleBuilder } from "@/components/ui/alert-rule-builder";

interface Alert {
  id: string;
  kind: string;
  severity: "INFO" | "WARNING" | "CRITICAL";
  title: string;
  detail: string | null;
  resource_type: string | null;
  resource_id: string | null;
  status: "OPEN" | "ACKNOWLEDGED" | "RESOLVED";
  created_at: string;
}

interface AlertSummary {
  open: number;
  critical: number;
  warning: number;
  info: number;
}

const SEVERITIES = ["CRITICAL", "WARNING", "INFO"] as const;
const STATUSES = ["OPEN", "ACKNOWLEDGED", "RESOLVED"] as const;

const SEVERITY_TONE: Record<string, string> = {
  CRITICAL: "border-red-500/50 bg-red-500/10 text-red-600 dark:text-red-400",
  WARNING: "border-amber-500/50 bg-amber-500/10 text-amber-700 dark:text-amber-400",
  INFO: "border-border text-muted-foreground",
};

function SeverityIcon({ severity }: { severity: string }) {
  if (severity === "CRITICAL") return <AlertTriangle className="size-3" aria-hidden />;
  if (severity === "WARNING") return <AlertTriangle className="size-3" aria-hidden />;
  return <Info className="size-3" aria-hidden />;
}

export default function AlertsPage() {
  const [status, setStatus] = useState<string>("OPEN");
  const [severity, setSeverity] = useState<string>("");
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const toast = useToast();

  const query = new URLSearchParams({ limit: "100" });
  if (status) query.set("status", status);
  if (severity) query.set("severity", severity);

  const alerts = useList<Alert>(`/alerts?${query.toString()}`);
  const summary = useQuery<AlertSummary>("/alerts/summary");

  const rows = search
    ? alerts.rows.filter((alert) =>
        `${alert.title} ${alert.detail ?? ""} ${alert.kind}`
          .toLowerCase()
          .includes(search.toLowerCase()),
      )
    : alerts.rows;

  async function acknowledge(alert: Alert) {
    setBusy(alert.id);
    try {
      await api.post(`/alerts/${alert.id}/acknowledge`);
      toast.success("Alert acknowledged", {
        label: "View alerts",
        onClick: () => window.location.assign("/alerts"),
      });
      alerts.reload();
      summary.reload();
    } catch (error) {
      toast.error(
        "Could not acknowledge",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(null);
    }
  }

  const counts = summary.data;

  return (
    <div className="flex flex-col gap-4 p-6">
      <header>
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <Bell className="size-5 text-rose-500" aria-hidden />
          Alerts
        </h1>
        <p className="text-xs text-muted-foreground">
          Conditions worth an operator&apos;s attention, newest first.
        </p>
      </header>

      <AlertRuleBuilder />

      {/* Real counts from /alerts/summary, not tallied in the browser. */}
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        {(
          [
            { key: "critical", label: "Critical", value: counts?.critical, tone: "text-red-600 dark:text-red-400" },
            { key: "warning", label: "Warning", value: counts?.warning, tone: "text-amber-700 dark:text-amber-400" },
            { key: "info", label: "Info", value: counts?.info, tone: "text-muted-foreground" },
          ] as const
        ).map((card) => (
          <div
            key={card.key}
            className="rounded-lg border border-border p-3"
          >
            <p className="text-xs text-muted-foreground">{card.label}</p>
            <p className={cn("text-2xl font-semibold tabular-nums", card.tone)}>
              {card.value ?? "—"}
            </p>
          </div>
        ))}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <label className="sr-only" htmlFor="alert-search">
          Search alerts
        </label>
        <Input
          id="alert-search"
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search alerts"
          className="h-8 w-56"
        />

        <label className="sr-only" htmlFor="alert-status">
          Status
        </label>
        <select
          id="alert-status"
          value={status}
          onChange={(e) => setStatus(e.target.value)}
          className="h-8 rounded-md border border-border bg-background px-2 text-xs"
        >
          <option value="">All statuses</option>
          {STATUSES.map((value) => (
            <option key={value} value={value}>
              {value.toLowerCase()}
            </option>
          ))}
        </select>

        <label className="sr-only" htmlFor="alert-severity">
          Severity
        </label>
        <select
          id="alert-severity"
          value={severity}
          onChange={(e) => setSeverity(e.target.value)}
          className="h-8 rounded-md border border-border bg-background px-2 text-xs"
        >
          <option value="">All severities</option>
          {SEVERITIES.map((value) => (
            <option key={value} value={value}>
              {value.toLowerCase()}
            </option>
          ))}
        </select>

        <span className="ml-auto flex items-center gap-1 text-[11px] text-muted-foreground">
          <Filter className="size-3" aria-hidden />
          {alerts.rows.length} shown
        </span>
      </div>

      <AsyncBoundary
        state={alerts.state}
        error={alerts.error}
        forbidden={alerts.forbidden}
        empty={alerts.empty}
        onRetry={alerts.reload}
        loadingLabel="Loading alerts"
        emptyTitle={
          status === "OPEN"
            ? "No open alerts"
            : "No alerts match these filters"
        }
        emptyDescription={
          status === "OPEN"
            ? "Nothing needs attention right now."
            : "Clear the filters to see every alert."
        }
      >
        <ul className="flex flex-col gap-2">
          {rows.map((alert) => (
            <li
              key={alert.id}
              className="flex flex-wrap items-center gap-3 rounded-lg border border-border p-3"
            >
              {/* Severity is spelled out, never colour-only (UI.md section 64). */}
              <Badge
                variant="outline"
                className={cn("gap-1 text-[10px]", SEVERITY_TONE[alert.severity])}
              >
                <SeverityIcon severity={alert.severity} />
                {alert.severity}
              </Badge>

              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium">{alert.title}</p>
                {alert.detail ? (
                  <p className="text-xs text-muted-foreground">{alert.detail}</p>
                ) : null}
                <p className="mt-0.5 text-[11px] text-muted-foreground">
                  {alert.kind.replace(/_/g, " ").toLowerCase()} ·{" "}
                  {formatRelative(alert.created_at)} ·{" "}
                  {formatTimestamp(alert.created_at)}
                </p>
              </div>

              {alert.resource_type === "execution" && alert.resource_id ? (
                <Link
                  href={`/executions/${alert.resource_id}`}
                  className="text-xs text-muted-foreground underline hover:text-foreground"
                >
                  View execution
                </Link>
              ) : null}

              <Badge variant="secondary" className="text-[10px]">
                {alert.status.toLowerCase()}
              </Badge>

              {alert.status === "OPEN" ? (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy === alert.id}
                  onClick={() => acknowledge(alert)}
                >
                  {busy === alert.id ? (
                    "Acknowledging…"
                  ) : (
                    <>
                      <CheckCircle2 className="mr-1 size-3.5" aria-hidden />
                      Acknowledge
                    </>
                  )}
                </Button>
              ) : null}
            </li>
          ))}
        </ul>

        {rows.length === 0 && search ? (
          <p className="py-6 text-center text-xs text-muted-foreground">
            No alerts match “{search}”.
          </p>
        ) : null}
      </AsyncBoundary>
    </div>
  );
}
