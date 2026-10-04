"use client";

/**
 * Incident detail (UI.md sections 29, 67).
 *
 * The incident, the alerts folded into it, and the timeline — all stored rows.
 * The share button produces the deep link an operator would paste into chat.
 */

import Link from "next/link";
import { use } from "react";
import { ArrowLeft, Share2 } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { copyText } from "@/lib/clipboard";
import { formatRelative, formatTimestamp } from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "cn";

interface AlertRow {
  id: string;
  kind: string;
  severity: "INFO" | "WARNING" | "CRITICAL";
  title: string;
  status: string;
  resource_type: string | null;
  resource_id: string | null;
}

interface TimelineEvent {
  id: string;
  at: string;
  kind: string;
  message: string;
  actor_id: string | null;
}

interface IncidentDetail {
  incident: {
    id: string;
    title: string;
    summary: string | null;
    severity: "SEV1" | "SEV2" | "SEV3";
    status: "OPEN" | "ACKNOWLEDGED" | "RESOLVED";
    impact: string | null;
    root_cause: string | null;
    resolution: string | null;
    detected_at: string;
    acknowledged_at: string | null;
    resolved_at: string | null;
  };
  alerts: AlertRow[];
  timeline: TimelineEvent[];
}

const SEVERITY_TONE: Record<string, string> = {
  SEV1: "border-red-500/50 bg-red-500/10 text-red-600 dark:text-red-400",
  SEV2: "border-amber-500/50 bg-amber-500/10 text-amber-700 dark:text-amber-400",
  SEV3: "border-border text-muted-foreground",
};

export default function IncidentDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const toast = useToast();
  const detail = useQuery<IncidentDetail>(`/incidents/${id}`);

  const incident = detail.data?.incident;
  const alerts = detail.data?.alerts ?? [];
  const timeline = detail.data?.timeline ?? [];

  async function share() {
    const ok = await copyText(window.location.href);
    if (ok) toast.success("Incident link copied");
    else toast.error("Could not copy link");
  }

  return (
    <AsyncBoundary
      state={detail.state}
      error={detail.error}
      forbidden={detail.forbidden}
      empty={false}
      onRetry={detail.reload}
      loadingLabel="Loading incident"
    >
      {incident ? (
        <div className="flex flex-col gap-4 p-6">
          <header className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <Link
                href="/incidents"
                className="mb-1 inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
              >
                <ArrowLeft className="size-3" aria-hidden />
                All incidents
              </Link>
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="text-lg font-semibold">{incident.title}</h1>
                <Badge
                  variant="outline"
                  className={cn("text-[10px]", SEVERITY_TONE[incident.severity])}
                >
                  {incident.severity}
                </Badge>
                <Badge variant="secondary" className="text-[10px]">
                  {incident.status.toLowerCase()}
                </Badge>
              </div>
              {incident.summary ? (
                <p className="mt-1 max-w-2xl text-xs text-muted-foreground">
                  {incident.summary}
                </p>
              ) : null}
              <p className="mt-1 text-[11px] text-muted-foreground">
                detected {formatRelative(incident.detected_at)} ·{" "}
                {formatTimestamp(incident.detected_at)}
              </p>
            </div>

            <Button variant="outline" size="sm" onClick={share}>
              <Share2 className="mr-1 size-3.5" aria-hidden />
              Copy link
            </Button>
          </header>

          <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Impact</CardTitle>
              </CardHeader>
              <CardContent className="text-xs">
                {incident.impact ?? "Not recorded."}
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Root cause</CardTitle>
              </CardHeader>
              <CardContent className="text-xs">
                {incident.root_cause ?? "Not recorded."}
              </CardContent>
            </Card>
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Resolution</CardTitle>
              </CardHeader>
              <CardContent className="text-xs">
                {incident.resolution ?? "Not recorded."}
              </CardContent>
            </Card>
          </div>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">
                Related alerts
                <span className="ml-2 font-normal text-muted-foreground">
                  {alerts.length}
                </span>
              </CardTitle>
            </CardHeader>
            <CardContent>
              {alerts.length === 0 ? (
                <EmptyState
                  title="No alerts attached"
                  description="This incident has no linked alerts."
                />
              ) : (
                <ul className="flex flex-col gap-1">
                  {alerts.map((alert) => (
                    <li
                      key={alert.id}
                      className="flex flex-wrap items-center gap-2 rounded px-2 py-1.5 text-xs hover:bg-accent/50"
                    >
                      <Badge
                        variant="outline"
                        className={cn(
                          "text-[10px]",
                          SEVERITY_TONE[
                            alert.severity === "CRITICAL"
                              ? "SEV1"
                              : alert.severity === "WARNING"
                                ? "SEV2"
                                : "SEV3"
                          ],
                        )}
                      >
                        {alert.severity}
                      </Badge>
                      <span className="font-medium">{alert.title}</span>
                      {alert.resource_type === "execution" && alert.resource_id ? (
                        <Link
                          href={`/executions/${alert.resource_id}`}
                          className="text-[11px] underline"
                        >
                          execution
                        </Link>
                      ) : null}
                      <span className="ml-auto text-[11px] text-muted-foreground">
                        {alert.status.toLowerCase()}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Timeline</CardTitle>
            </CardHeader>
            <CardContent>
              {timeline.length === 0 ? (
                <EmptyState
                  title="No timeline entries"
                  description="Nothing has been recorded against this incident."
                />
              ) : (
                <ol className="flex flex-col gap-2 border-l border-border pl-4">
                  {timeline.map((event) => (
                    <li key={event.id} className="relative text-xs">
                      <span
                        className="absolute -left-[1.3rem] top-1 size-2 rounded-full bg-border"
                        aria-hidden
                      />
                      <div className="flex flex-wrap items-center gap-2">
                        <Badge variant="secondary" className="text-[10px]">
                          {event.kind.toLowerCase()}
                        </Badge>
                        <span className="text-muted-foreground">
                          {formatTimestamp(event.at)}
                        </span>
                      </div>
                      <p className="mt-0.5">{event.message}</p>
                    </li>
                  ))}
                </ol>
              )}
            </CardContent>
          </Card>
        </div>
      ) : (
        <EmptyState
          title="Incident not found"
          description="It may have been deleted, or it belongs to another tenant."
        />
      )}
    </AsyncBoundary>
  );
}
