"use client";

/**
 * "What's going to run?" (UI.md section 22).
 *
 * A forward-looking timeline. Merges executions that already have a scheduled
 * time with the schedules that will produce the next ones, so the timeline is
 * useful at 09:00 for a 02:00 job that has not run yet today.
 */

import Link from "next/link";
import { CalendarClock, Clock } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { formatTimestamp } from "@/lib/types";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { StatusBadge } from "@/components/status-badge";
import { Badge } from "@/components/ui/badge";

interface Item {
  kind: "execution" | "schedule";
  id: string;
  job_id: string;
  at: string;
  status?: string;
  priority?: string;
  expression?: string | null;
  timezone?: string;
}

interface Upcoming {
  items: Item[];
  count: number;
}

export default function UpcomingPage() {
  const upcoming = useQuery<Upcoming>("/upcoming");

  return (
    <div className="flex flex-col gap-4 p-6">
      <header>
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <Clock className="size-4" aria-hidden />
          What's going to run?
        </h1>
        <p className="text-xs text-muted-foreground">
          The next scheduled executions and the schedules that will produce them, in time order.
        </p>
      </header>

      <AsyncBoundary
        state={upcoming.state}
        error={upcoming.error}
        forbidden={upcoming.forbidden}
        empty={upcoming.data ? upcoming.data.count === 0 : false}
        onRetry={upcoming.reload}
        loadingLabel="Loading upcoming runs"
        emptyTitle="Nothing scheduled"
        emptyDescription="Create a schedule and it will appear here with its next run time."
      >
        <ol className="flex flex-col gap-2">
          {(upcoming.data?.items ?? []).map((item) => (
            <li key={`${item.kind}-${item.id}`}>
              <div className="flex flex-wrap items-center gap-3 rounded-lg border border-border px-3 py-2 text-xs">
                <span className="w-44 shrink-0 tabular-nums">
                  {formatTimestamp(item.at)}
                </span>

                {item.kind === "execution" ? (
                  <>
                    <Link
                      href={`/executions/${item.id}`}
                      className="font-mono text-muted-foreground underline-offset-4 hover:underline"
                    >
                      {item.id.slice(0, 8)}
                    </Link>
                    {item.status ? <StatusBadge status={item.status} /> : null}
                    {item.priority ? (
                      <Badge variant="secondary" className="text-[10px]">
                        {item.priority.toLowerCase()}
                      </Badge>
                    ) : null}
                  </>
                ) : (
                  <>
                    <code className="text-muted-foreground">
                      {item.expression ?? "schedule"}
                    </code>
                    <Badge variant="secondary" className="text-[10px]">
                      {item.timezone}
                    </Badge>
                    <span className="flex items-center gap-1 text-[11px] text-muted-foreground">
                      <CalendarClock className="size-3" aria-hidden />
                      scheduled run
                    </span>
                  </>
                )}

                <Link
                  href={`/jobs/${item.job_id}`}
                  className="ml-auto text-[11px] text-muted-foreground underline-offset-4 hover:underline"
                >
                  job
                </Link>
              </div>
            </li>
          ))}
        </ol>
      </AsyncBoundary>
    </div>
  );
}
