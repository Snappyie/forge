"use client";

/**
 * Calendar (UI.md section 20).
 *
 * Month, week and day views over real executions and schedules. Occupancy is
 * derived from stored rows; nothing is invented to fill the grid.
 */

import Link from "next/link";
import { useMemo, useState } from "react";
import { CalendarClock, ChevronLeft, ChevronRight } from "lucide-react";

import { useList } from "@/lib/useQuery";
import { formatTimestamp, type Execution, type Schedule } from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { Button } from "@/components/ui/button";
import { cn } from "cn";

type View = "month" | "week" | "day";

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** Midnight UTC on the day containing `value`. */
function startOfDay(value: Date): Date {
  return new Date(
    Date.UTC(value.getUTCFullYear(), value.getUTCMonth(), value.getUTCDate()),
  );
}

/** The Monday of the week containing `value`. */
function startOfWeek(value: Date): Date {
  const day = startOfDay(value);
  // `getUTCDay` is 0 for Sunday; shift so Monday is the first column.
  const offset = (day.getUTCDay() + 6) % 7;
  day.setUTCDate(day.getUTCDate() - offset);
  return day;
}

function addDays(value: Date, days: number): Date {
  const next = new Date(value);
  next.setUTCDate(next.getUTCDate() + days);
  return next;
}

function isoDay(value: Date): string {
  return value.toISOString().slice(0, 10);
}

export default function CalendarPage() {
  const [view, setView] = useState<View>("month");
  const [anchor, setAnchor] = useState<Date>(() => startOfDay(new Date()));

  const executions = useList<Execution>("/executions?limit=200");
  const schedules = useList<Schedule>("/schedules?limit=100");

  const range = useMemo(() => {
    if (view === "day") return { start: anchor, days: 1 };
    if (view === "week") return { start: startOfWeek(anchor), days: 7 };
    // Month: pad to whole weeks so the grid stays rectangular.
    const first = new Date(
      Date.UTC(anchor.getUTCFullYear(), anchor.getUTCMonth(), 1),
    );
    return { start: startOfWeek(first), days: 42 };
  }, [view, anchor]);

  const days = useMemo(
    () => Array.from({ length: range.days }, (_, i) => addDays(range.start, i)),
    [range],
  );

  // Index executions by calendar day so a cell can show its own rows.
  const byDay = useMemo(() => {
    const map = new Map<string, Execution[]>();
    for (const execution of executions.rows) {
      const at = execution.started_at ?? execution.created_at;
      const key = at.slice(0, 10);
      map.set(key, [...(map.get(key) ?? []), execution]);
    }
    return map;
  }, [executions.rows]);

  const dueByDay = useMemo(() => {
    const map = new Map<string, Schedule[]>();
    for (const schedule of schedules.rows) {
      if (!schedule.next_run_at) continue;
      const key = schedule.next_run_at.slice(0, 10);
      map.set(key, [...(map.get(key) ?? []), schedule]);
    }
    return map;
  }, [schedules.rows]);

  const loading = executions.state === "loading" || schedules.state === "loading";
  const error = executions.error ?? schedules.error;
  const empty = schedules.empty && executions.empty;

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Calendar</h1>
          <p className="text-xs text-muted-foreground">
            Schedules due and executions that ran, in UTC.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <div className="flex rounded-md border border-border">
            {(["month", "week", "day"] as View[]).map((option) => (
              <button
                key={option}
                type="button"
                onClick={() => setView(option)}
                aria-pressed={view === option}
                className={cn(
                  "px-3 py-1 text-xs capitalize",
                  view === option
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent/50",
                )}
              >
                {option}
              </button>
            ))}
          </div>

          <Button
            variant="outline"
            size="icon"
            aria-label="Previous period"
            onClick={() => setAnchor(shift(anchor, view, -1))}
          >
            <ChevronLeft className="size-4" aria-hidden />
          </Button>
          <Button
            variant="outline"
            size="icon"
            aria-label="Next period"
            onClick={() => setAnchor(shift(anchor, view, 1))}
          >
            <ChevronRight className="size-4" aria-hidden />
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setAnchor(startOfDay(new Date()))}
          >
            Today
          </Button>
        </div>
      </header>

      <AsyncBoundary
        state={loading ? "loading" : error ? "error" : "ready"}
        error={error}
        forbidden={executions.forbidden || schedules.forbidden}
        empty={empty}
        onRetry={() => {
          executions.reload();
          schedules.reload();
        }}
        loadingLabel="Loading calendar"
        emptyTitle="Nothing scheduled or executed"
        emptyDescription="Create a schedule, or run a job, and it will appear here."
      >
        {view !== "day" ? (
          <div className="mb-1 grid grid-cols-7 gap-px text-center text-[11px] uppercase tracking-wide text-muted-foreground">
            {WEEKDAYS.map((day) => (
              <span key={day}>{day}</span>
            ))}
          </div>
        ) : null}

        <div
          className={cn(
            "grid gap-px overflow-hidden rounded-lg border border-border bg-border",
            view !== "day" ? "grid-cols-7" : "grid-cols-1",
          )}
        >
          {days.map((day) => {
            const key = isoDay(day);
            const runs = byDay.get(key) ?? [];
            const due = dueByDay.get(key) ?? [];
            const today = key === isoDay(new Date());

            return (
              <div
                key={key}
                className={cn(
                  "min-h-[6rem] bg-background p-1.5",
                  view === "day" && "min-h-[12rem]",
                  today && "ring-1 ring-inset ring-primary/40",
                )}
              >
                <div className="mb-1 flex items-center justify-between">
                  <span
                    className={cn(
                      "text-[11px] tabular-nums",
                      today ? "font-semibold text-primary" : "text-muted-foreground",
                    )}
                  >
                    {day.getUTCDate()}
                  </span>
                  {due.length > 0 ? (
                    <span
                      className="text-muted-foreground"
                      title={`${due.length} schedule(s) due`}
                    >
                      <CalendarClock className="size-3" aria-hidden />
                    </span>
                  ) : null}
                </div>

                <div className="flex flex-col gap-0.5">
                  {due.slice(0, 2).map((schedule) => (
                    <span
                      key={schedule.id}
                      className="truncate rounded bg-amber-500/15 px-1 py-0.5 text-[10px] text-amber-700 dark:text-amber-400"
                      title={`${schedule.expression ?? "schedule"} (${schedule.timezone}) due`}
                    >
                      {schedule.expression ?? "schedule"}
                    </span>
                  ))}
                  {runs.slice(0, 3).map((execution) => (
                    <Link
                      key={execution.id}
                      href={`/executions/${execution.id}`}
                      className="truncate rounded px-1 py-0.5 text-[10px] hover:bg-accent/60"
                      title={`${execution.status} · ${formatTimestamp(execution.created_at)}`}
                    >
                      {execution.status.toLowerCase()}
                    </Link>
                  ))}
                  {runs.length > 3 ? (
                    <span className="text-[10px] text-muted-foreground">
                      +{runs.length - 3} more
                    </span>
                  ) : null}
                </div>
              </div>
            );
          })}
        </div>

        {/* A legend: colour alone must not carry the meaning (UI.md section 64). */}
        <div className="flex flex-wrap items-center gap-3 text-[11px] text-muted-foreground">
          <span className="flex items-center gap-1">
            <span className="size-2 rounded-sm bg-amber-500/40" aria-hidden /> schedule due
          </span>
          <span className="flex items-center gap-1">
            <span className="size-2 rounded-sm bg-accent" aria-hidden /> execution
          </span>
          <span className="ml-auto">
            Showing {days.length} day{days.length === 1 ? "" : "s"} from{" "}
            {formatTimestamp(days[0]?.toISOString())}
          </span>
        </div>
      </AsyncBoundary>
    </div>
  );
}

/** Moves the anchor by one period in the given direction. */
function shift(anchor: Date, view: View, direction: 1 | -1): Date {
  if (view === "day") return addDays(anchor, direction);
  if (view === "week") return addDays(anchor, 7 * direction);

  const next = new Date(anchor);
  next.setUTCMonth(next.getUTCMonth() + direction);
  return startOfDay(next);
}
