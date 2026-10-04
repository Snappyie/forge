"use client";

/**
 * Alerts (UI.md section 28).
 *
 * A central inbox. Reads the real feed and the real summary counts, and
 * acknowledging calls the acknowledge endpoint, so an alert's status reflects an
 * actual action rather than a local toggle.
 *
 * Alerts are listed as rows, not as cards. An inbox is scanned, and a screen of
 * bordered cards pushes each row twice as tall for the same information.
 */

import { useState } from "react";
import { Check, RefreshCw, Search } from "lucide-react";

import { useList, useQuery } from "@/lib/useQuery";
import { api, ApiError } from "@/lib/api";
import { formatRelative, formatTimestamp } from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { TableBody, TableHeader, TableRow } from "@/components/ui/table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  DataTable,
  DataTableCell,
  DataTableHead,
  PageHeader,
  RowLink,
  Stat,
  Toolbar,
  TableFooter,
  TableSkeleton,
} from "@/components/page";
import { AlertRuleBuilder } from "@/components/ui/alert-rule-builder";
import { SeverityTag, StatusCell } from "@/components/status-badge";
import { EmptyState, ErrorState, ForbiddenState } from "@/components/states";

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

const STATUSES = ["OPEN", "ACKNOWLEDGED", "RESOLVED"] as const;
const SEVERITIES = ["CRITICAL", "WARNING", "INFO"] as const;

export default function AlertsPage() {
  const [status, setStatus] = useState<string>("OPEN");
  const [severity, setSeverity] = useState<string>("ALL");
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const toast = useToast();

  const query = new URLSearchParams({ limit: "100" });
  if (status !== "ALL") query.set("status", status);
  if (severity !== "ALL") query.set("severity", severity);

  const alerts = useList<Alert>(`/alerts?${query.toString()}`);
  const summary = useQuery<AlertSummary>("/alerts/summary");
  const counts = summary.data;

  const rows = search.trim()
    ? alerts.rows.filter((alert) =>
        `${alert.title} ${alert.detail ?? ""} ${alert.kind}`
          .toLowerCase()
          .includes(search.trim().toLowerCase()),
      )
    : alerts.rows;

  const filtering =
    status !== "ALL" || severity !== "ALL" || search.trim().length > 0;

  async function acknowledge(alert: Alert) {
    setBusy(alert.id);
    try {
      await api.post(`/alerts/${alert.id}/acknowledge`);
      toast.success(`Acknowledged: ${alert.title}`);
      alerts.reload();
      summary.reload();
    } catch (error) {
      toast.error(
        "Could not acknowledge",
        error instanceof ApiError ? error.message : undefined,
      );
    } finally {
      setBusy(null);
    }
  }

  function clearFilters() {
    setStatus("ALL");
    setSeverity("ALL");
    setSearch("");
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Alerts"
        description="Conditions worth an operator's attention, newest first."
        actions={
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              alerts.reload();
              summary.reload();
            }}
            disabled={alerts.state === "loading"}
          >
            <RefreshCw
              className={alerts.state === "loading" ? "animate-spin" : undefined}
              aria-hidden
            />
            Refresh
          </Button>
        }
      />

      {/* Real counts from /alerts/summary, not tallied in the browser. */}
      <div className="grid grid-cols-2 gap-2 border-b border-border p-3 sm:grid-cols-4">
        <Stat label="Open" value={counts?.open ?? null} />
        <Stat
          label="Critical"
          value={counts?.critical ?? null}
          tone={counts?.critical ? "danger" : "neutral"}
        />
        <Stat
          label="Warning"
          value={counts?.warning ?? null}
          tone={counts?.warning ? "warning" : "neutral"}
        />
        <Stat label="Info" value={counts?.info ?? null} />
      </div>

      <Toolbar>
        <div className="relative min-w-[14rem] flex-1 sm:max-w-xs">
          <Search
            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
            aria-hidden
          />
          <Input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Filter alerts"
            aria-label="Filter alerts"
            className="h-7 pl-7 text-[12.5px]"
          />
        </div>

        <Select value={status} onValueChange={(value) => setStatus(value ?? "OPEN")}>
          <SelectTrigger
            size="sm"
            className="h-7 w-36 text-[12.5px]"
            aria-label="Filter by status"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="ALL">Any status</SelectItem>
            {STATUSES.map((value) => (
              <SelectItem key={value} value={value}>
                {value.charAt(0) + value.slice(1).toLowerCase()}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Select value={severity} onValueChange={(value) => setSeverity(value ?? "ALL")}>
          <SelectTrigger
            size="sm"
            className="h-7 w-36 text-[12.5px]"
            aria-label="Filter by severity"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="ALL">Any severity</SelectItem>
            {SEVERITIES.map((value) => (
              <SelectItem key={value} value={value}>
                {value.charAt(0) + value.slice(1).toLowerCase()}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {filtering ? (
          <Button
            variant="ghost"
            size="sm"
            onClick={clearFilters}
            className="h-7 text-[12.5px] text-muted-foreground"
          >
            Clear filters
          </Button>
        ) : null}

        <div className="ml-auto">
          <AlertRuleBuilder />
        </div>
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-auto">
        {alerts.state === "loading" ? (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead className="w-28">Severity</DataTableHead>
                <DataTableHead>Alert</DataTableHead>
                <DataTableHead className="w-28">Status</DataTableHead>
                <DataTableHead className="w-32" align="right">
                  Raised
                </DataTableHead>
                <DataTableHead className="w-24" align="right">
                  Action
                </DataTableHead>
              </TableRow>
            </TableHeader>
            <TableSkeleton rows={8} columns={5} />
          </DataTable>
        ) : alerts.state === "error" ? (
          alerts.forbidden ? (
            <ForbiddenState />
          ) : (
            <ErrorState error={alerts.error} onRetry={alerts.reload} />
          )
        ) : rows.length === 0 ? (
          <EmptyState
            title={
              filtering ? "No alerts match these filters" : "No open alerts"
            }
            description={
              filtering
                ? "Clear the filters to see every alert."
                : "Nothing needs attention right now. Alerts appear here as conditions fire."
            }
            action={
              filtering ? (
                <Button size="sm" variant="outline" onClick={clearFilters}>
                  Clear filters
                </Button>
              ) : null
            }
          />
        ) : (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead className="w-28">Severity</DataTableHead>
                <DataTableHead>Alert</DataTableHead>
                <DataTableHead className="w-28">Status</DataTableHead>
                <DataTableHead className="w-32" align="right">
                  Raised
                </DataTableHead>
                <DataTableHead className="w-28" align="right">
                  Action
                </DataTableHead>
              </TableRow>
            </TableHeader>

            <TableBody>
              {rows.map((alert) => (
                <TableRow key={alert.id} className="h-9">
                  <DataTableCell>
                    <SeverityTag severity={alert.severity} />
                  </DataTableCell>

                  <DataTableCell>
                    <span className="font-medium">{alert.title}</span>
                    {alert.detail ? (
                      <span
                        className="mt-0.5 block max-w-[40rem] truncate text-[12px] text-muted-foreground"
                        title={alert.detail}
                      >
                        {alert.detail}
                      </span>
                    ) : null}
                    <span className="mt-0.5 block text-[11px] text-muted-foreground">
                      {alert.kind.replace(/_/g, " ").toLowerCase()}
                      {alert.resource_id ? (
                        <>
                          {" · "}
                          {alert.resource_type === "execution" ? (
                            <RowLink href={`/executions/${alert.resource_id}`}>
                              view execution
                            </RowLink>
                          ) : alert.resource_type === "job" ? (
                            <RowLink href={`/jobs/${alert.resource_id}`}>
                              view job
                            </RowLink>
                          ) : (
                            <code className="font-mono">
                              {alert.resource_id.slice(0, 8)}
                            </code>
                          )}
                        </>
                      ) : null}
                    </span>
                  </DataTableCell>

                  <DataTableCell>
                    <StatusCell status={alert.status} />
                  </DataTableCell>

                  <DataTableCell
                    align="right"
                    className="text-[12px] text-muted-foreground"
                    title={formatTimestamp(alert.created_at)}
                  >
                    {formatRelative(alert.created_at)}
                  </DataTableCell>

                  <DataTableCell align="right">
                    {alert.status === "OPEN" ? (
                      <Button
                        variant="outline"
                        size="sm"
                        className="h-7 text-[12.5px]"
                        disabled={busy === alert.id}
                        onClick={() => acknowledge(alert)}
                      >
                        <Check aria-hidden />
                        {busy === alert.id ? "Working…" : "Acknowledge"}
                      </Button>
                    ) : (
                      <span className="text-[11.5px] text-muted-foreground">
                        {alert.status === "ACKNOWLEDGED" ? "Acknowledged" : "Resolved"}
                      </span>
                    )}
                  </DataTableCell>
                </TableRow>
              ))}
            </TableBody>
          </DataTable>
        )}
      </div>

      <TableFooter shown={rows.length} total={alerts.rows.length} />
    </div>
  );
}