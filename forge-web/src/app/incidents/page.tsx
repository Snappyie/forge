"use client";

/**
 * Incidents (UI.md section 29).
 *
 * Lists real incidents. The count column comes from the API, not from a
 * placeholder, because an incident page that invents its own severity is worse
 * than no incident page.
 */

import Link from "next/link";
import { useList } from "@/lib/useQuery";
import { formatRelative, formatTimestamp } from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { Badge } from "@/components/ui/badge";
import { cn } from "cn";

interface Incident {
  id: string;
  title: string;
  summary: string | null;
  severity: "SEV1" | "SEV2" | "SEV3";
  status: "OPEN" | "ACKNOWLEDGED" | "RESOLVED";
  alert_count: number | null;
  impact: string | null;
  detected_at: string;
  acknowledged_at: string | null;
  resolved_at: string | null;
}

const SEVERITY_TONE: Record<string, string> = {
  SEV1: "border-red-500/50 bg-red-500/10 text-red-600 dark:text-red-400",
  SEV2: "border-amber-500/50 bg-amber-500/10 text-amber-700 dark:text-amber-400",
  SEV3: "border-border text-muted-foreground",
};

export default function IncidentsPage() {
  const incidents = useList<Incident>("/incidents?limit=50");

  return (
    <div className="flex flex-col gap-4 p-6">
      <header>
        <h1 className="text-lg font-semibold">Incidents</h1>
        <p className="text-xs text-muted-foreground">
          Operational problems grouping related alerts, newest first.
        </p>
      </header>

      <AsyncBoundary
        state={incidents.state}
        error={incidents.error}
        forbidden={incidents.forbidden}
        empty={incidents.empty}
        onRetry={incidents.reload}
        loadingLabel="Loading incidents"
        emptyTitle="No incidents recorded"
        emptyDescription="Incidents appear here when alerts are grouped into one operational problem."
      >
        <ul className="flex flex-col gap-2">
          {incidents.rows.map((incident) => (
            <li key={incident.id}>
              <Link
                href={`/incidents/${incident.id}`}
                className="flex flex-col gap-1 rounded-lg border border-border p-3 transition-colors hover:bg-accent/40"
              >
                <div className="flex flex-wrap items-center gap-2">
                  {/* Severity is spelled out, not colour-only (UI.md section 64). */}
                  <Badge
                    variant="outline"
                    className={cn("text-[10px]", SEVERITY_TONE[incident.severity])}
                  >
                    {incident.severity}
                  </Badge>
                  <span className="text-sm font-medium">{incident.title}</span>
                  <Badge variant="secondary" className="text-[10px]">
                    {incident.status.toLowerCase()}
                  </Badge>
                </div>

                {incident.summary ? (
                  <p className="text-xs text-muted-foreground">{incident.summary}</p>
                ) : null}

                <p className="text-[11px] text-muted-foreground">
                  detected {formatRelative(incident.detected_at)} ·{" "}
                  {formatTimestamp(incident.detected_at)}
                  {incident.alert_count
                    ? ` · ${incident.alert_count} alert${incident.alert_count === 1 ? "" : "s"}`
                    : ""}
                </p>
              </Link>
            </li>
          ))}
        </ul>
      </AsyncBoundary>
    </div>
  );
}
