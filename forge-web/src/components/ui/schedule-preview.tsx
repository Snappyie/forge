"use client";

/**
 * Next-runs preview for an unsaved schedule (UI.md sections 8 and 13).
 *
 * This is the highest-value control on the schedule step, and it is the reason
 * `POST /schedules/explain` exists: `/schedules/{id}/preview` needs a saved
 * schedule, which cannot help while authoring one. The explain endpoint runs the
 * same `forge_scheduler::preview_occurrences` the scheduler itself uses, so what
 * is shown here is what will fire.
 *
 * Two deliberate choices:
 *
 * - Occurrences are shown in both the schedule's own timezone and UTC. An
 *   operator comparing against an incident report in UTC should not have to do
 *   mental arithmetic, and the two columns are where a DST surprise shows up.
 * - An invalid expression reports the server's message rather than hiding the
 *   preview. A silent empty table reads as "this schedule never runs", which is
 *   the most dangerous thing this component could imply.
 */

import { useEffect, useRef, useState } from "react";
import { AlertTriangle, CheckCircle2, Loader2 } from "lucide-react";

import { api } from "@/lib/api";
import { cn } from "cn";
import { Badge } from "@/components/ui/badge";

interface Anomaly {
  at: string;
  kind: "NONEXISTENT_LOCAL_TIME" | "AMBIGUOUS_LOCAL_TIME";
  note: string;
}

interface ExplainResponse {
  expression: string;
  timezone: string;
  next_run_at: string | null;
  occurrences: string[];
  anomalies: Anomaly[];
}

type PreviewState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ready"; data: ExplainResponse }
  | { status: "error"; message: string };

const WEEKDAYS = [
  "Sunday",
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
];

/** Formats an instant in the schedule's own timezone, with its abbreviation. */
function formatLocal(iso: string, timezone: string): { date: string; time: string; zone: string } {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return { date: "-", time: "-", zone: "" };

  let parts: Intl.DateTimeFormatPart[];
  try {
    parts = new Intl.DateTimeFormat("en-GB", {
      timeZone: timezone,
      weekday: "short",
      day: "numeric",
      month: "short",
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
      timeZoneName: "short",
    }).formatToParts(date);
  } catch {
    return { date: "-", time: "-", zone: "" };
  }

  const get = (type: string) => parts.find((p) => p.type === type)?.value ?? "";
  return {
    date: `${get("weekday")}, ${get("day")} ${get("month")}`,
    time: `${get("hour")}:${get("minute")}`,
    zone: get("timeZoneName"),
  };
}

function formatUtc(iso: string): { date: string; time: string } {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return { date: "-", time: "-" };
  return {
    date: `${date.toISOString().slice(8, 10)} ${date.toLocaleString("en-GB", {
      month: "short",
      timeZone: "UTC",
    })}`,
    time: date.toISOString().slice(11, 16),
  };
}

/** Coarse "how long until" for the relative column. */
function untilLabel(iso: string): string {
  const ms = new Date(iso).getTime() - Date.now();
  if (Number.isNaN(ms)) return "-";
  const minutes = Math.round(ms / 60_000);
  if (minutes < 60) return `${Math.max(0, minutes)}m`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `${hours}h`;
  return `${Math.round(hours / 24)}d`;
}

/**
 * Describes a cron expression in plain English.
 *
 * Only the shapes the builder can produce are described; anything else returns
 * null and the caller falls back to the structural hint. Guessing at an
 * arbitrary expression would be worse than saying nothing.
 */
export function describeCron(expression: string): string | null {
  const fields = expression.trim().split(/\s+/);
  if (fields.length !== 5) return null;
  const [minute, hour, dom, month, dow] = fields;

  const time =
    /^\d+$/.test(hour) && /^\d+$/.test(minute)
      ? `${hour.padStart(2, "0")}:${minute.padStart(2, "0")}`
      : null;

  const dayOfWeek = (value: string): string | null => {
    if (value === "*") return null;
    if (/^\d$/.test(value)) return WEEKDAYS[Number(value)];
    if (/^\d-\d$/.test(value)) {
      const [from, to] = value.split("-").map(Number);
      return `${WEEKDAYS[from]} through ${WEEKDAYS[to]}`;
    }
    return null;
  };

  const days = dayOfWeek(dow);
  const everyDay = dom === "*" && month === "*";

  if (everyDay && days) return time ? `At ${time}, ${days.toLowerCase()}` : `${days}`;
  if (everyDay && time) return `At ${time}`;
  if (everyDay && minute === "*" && /^\d+$/.test(hour ?? ""))
    return `Every ${WEEKDAYS[Number(hour)]}`;
  if (/^\*\/\d+$/.test(minute)) {
    const every = Number(minute.split("/")[1]);
    return time && !/^\d+$/.test(hour ?? "") && hour !== "*"
      ? `Every ${every} minutes`
      : `Every ${every} minutes`;
  }
  return null;
}

export function SchedulePreview({
  expression,
  timezone,
  count = 6,
}: {
  expression: string;
  timezone: string;
  count?: number;
}) {
  const [state, setState] = useState<PreviewState>({ status: "idle" });
  // Guards against a slow response for an old expression overwriting a newer
  // one, which would show runs for a schedule the author has already changed.
  const requestRef = useRef(0);

  useEffect(() => {
    const expressionValue = expression.trim();
    const zone = timezone.trim();

    if (!expressionValue || !zone) {
      setState({ status: "idle" });
      return;
    }

    const ticket = ++requestRef.current;
    setState({ status: "loading" });

    let cancelled = false;
    api
      .post<ExplainResponse>("/schedules/explain", {
        expression: expressionValue,
        timezone: zone,
        count,
      })
      .then((data) => {
        if (cancelled || ticket !== requestRef.current) return;
        setState({ status: "ready", data });
      })
      .catch((error: unknown) => {
        if (cancelled || ticket !== requestRef.current) return;
        setState({
          status: "error",
          message:
            error instanceof Error
              ? error.message
              : "the expression could not be evaluated",
        });
      });

    return () => {
      cancelled = true;
    };
  }, [expression, timezone, count]);

  if (state.status === "idle") {
    return (
      <div className="rounded-lg border border-border p-4 text-xs text-muted-foreground">
        Enter an expression and a timezone to see when this will run.
      </div>
    );
  }

  if (state.status === "loading") {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-border p-4 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" aria-hidden />
        Working out the next runs
      </div>
    );
  }

  if (state.status === "error") {
    return (
      <div
        role="status"
        className="flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/10 p-3 text-xs"
      >
        <AlertTriangle
          className="mt-0.5 size-3.5 shrink-0 text-destructive"
          aria-hidden
        />
        <div>
          <p className="font-medium text-destructive">This schedule will not fire</p>
          <p className="mt-0.5 text-destructive/90">{state.message}</p>
        </div>
      </div>
    );
  }

  const { occurrences, anomalies } = state.data;
  const anomalyTimes = new Set(anomalies.map((a) => a.at));

  return (
    <div className="overflow-hidden rounded-lg border border-border">
      <div className="flex items-center gap-2 border-b border-border bg-muted/30 px-4 py-2.5">
        <p className="text-xs font-medium">Next {occurrences.length} runs</p>
        <span className="text-[11px] text-muted-foreground">in {state.data.timezone}</span>
        {anomalies.length === 0 ? (
          <Badge
            variant="secondary"
            className="ml-auto gap-1 text-[10px] text-emerald-600 dark:text-emerald-400"
          >
            <CheckCircle2 className="size-3" aria-hidden />
            no DST conflicts
          </Badge>
        ) : (
          <Badge
            variant="secondary"
            className="ml-auto gap-1 text-[10px] text-amber-600 dark:text-amber-400"
          >
            <AlertTriangle className="size-3" aria-hidden />
            {anomalies.length} DST {anomalies.length === 1 ? "conflict" : "conflicts"}
          </Badge>
        )}
      </div>

      {occurrences.length === 0 ? (
        <p className="px-4 py-6 text-center text-xs text-muted-foreground">
          This expression never fires. Check the day-of-month and month fields.
        </p>
      ) : (
        <table className="w-full text-xs">
          <thead>
            <tr className="border-b border-border text-left text-[10px] uppercase tracking-wide text-muted-foreground">
              <th className="px-4 py-2 font-medium">Local time</th>
              <th className="px-4 py-2 font-medium">UTC</th>
              <th className="px-4 py-2 text-right font-medium">In</th>
            </tr>
          </thead>
          <tbody>
            {occurrences.map((iso) => {
              const local = formatLocal(iso, state.data.timezone);
              const utc = formatUtc(iso);
              const flagged = anomalyTimes.has(iso);
              return (
                <tr key={iso} className="border-b border-border/60 last:border-0">
                  <td className="px-4 py-2 tabular-nums">
                    <span className="font-medium">{local.date}</span>{" "}
                    <span className="text-muted-foreground">{local.time}</span>{" "}
                    <span className="text-muted-foreground">{local.zone}</span>
                    {flagged ? (
                      <span
                        className="ml-1.5 inline-flex items-center gap-1 text-[10px] text-amber-600 dark:text-amber-400"
                        title="This local time does not exist, or occurs twice, on this date."
                      >
                        <AlertTriangle className="size-3" aria-hidden />
                        DST
                      </span>
                    ) : null}
                  </td>
                  <td className="px-4 py-2 font-mono text-[11px] tabular-nums text-muted-foreground">
                    {utc.date} {utc.time}
                  </td>
                  <td className="px-4 py-2 text-right tabular-nums text-muted-foreground">
                    {untilLabel(iso)}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}