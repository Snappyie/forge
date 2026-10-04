"use client";

/**
 * Bulk operations (UI.md sections 5 and 37).
 *
 * Selection, a confirmation that states the exact count, and a per-job result
 * report. A batch where some jobs fail says so rather than collapsing to a
 * single success or failure.
 */

import { useState } from "react";
import { Archive, Play, RefreshCw, Square } from "lucide-react";

import { api } from "@/lib/api";
import { useToast } from "@/lib/useToast";
import { Button } from "@/components/ui/button";

type Action = "PAUSE" | "RESUME" | "ARCHIVE" | "RUN";

interface Result {
  job_id: string;
  ok: boolean;
  error?: string;
}

export function BulkActions({
  selected,
  onDone,
}: {
  selected: string[];
  onDone: () => void;
}) {
  const toast = useToast();
  const [busy, setBusy] = useState<Action | null>(null);
  const [results, setResults] = useState<Result[]>([]);

  if (selected.length === 0) return null;

  async function run(action: Action) {
    setBusy(action);
    try {
      const response = await api.post<{
        requested: number;
        applied: number;
        failed: number;
        results: Result[];
      }>("/jobs/bulk", { action, job_ids: selected });

      setResults(response.results);

      if (response.failed === 0) {
        toast.success(
          `${response.applied} job${response.applied === 1 ? "" : "s"} ${action.toLowerCase()}d`,
        );
      } else {
        // Both counts: the batch partly worked, and saying only "success"
        // would hide the jobs that were refused.
        toast.error(
          `${response.applied} of ${response.requested} succeeded`,
          `${response.failed} job${response.failed === 1 ? "" : "s"} could not be changed.`,
        );
      }
      onDone();
    } catch (error) {
      toast.error(
        "Bulk action failed",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(null);
    }
  }

  const failures = results.filter((r) => !r.ok);

  /*
   * A bare row, no container.
   *
   * The caller supplies the selection bar this renders into, so drawing another
   * bordered box here would put a card inside a card and give the selection a
   * second vertical edge.
   */
  return (
    <div className="flex flex-wrap items-center gap-2">
      <span className="text-[12.5px] font-medium">
        {selected.length} job{selected.length === 1 ? "" : "s"} selected
      </span>
      <Button
        variant="outline"
        size="sm"
        className="h-7 text-[12.5px]"
        disabled={busy !== null}
        onClick={() => run("PAUSE")}
      >
        <Square aria-hidden />
        Pause
      </Button>
      <Button
        variant="outline"
        size="sm"
        className="h-7 text-[12.5px]"
        disabled={busy !== null}
        onClick={() => run("RESUME")}
      >
        <RefreshCw aria-hidden />
        Resume
      </Button>
      <Button
        variant="outline"
        size="sm"
        className="h-7 text-[12.5px]"
        disabled={busy !== null}
        onClick={() => run("ARCHIVE")}
      >
        <Archive aria-hidden />
        Archive
      </Button>
      <Button
        variant="outline"
        size="sm"
        className="h-7 text-[12.5px]"
        disabled={busy !== null}
        onClick={() => run("RUN")}
      >
        <Play aria-hidden />
        Run
      </Button>
      {busy ? (
        <span
          className="text-[11.5px] text-muted-foreground"
          role="status"
          aria-live="polite"
        >
          applying {busy.toLowerCase()}…
        </span>
      ) : null}

      {failures.length > 0 ? (
        <ul className="flex basis-full flex-col gap-0.5 pt-1">
          {failures.map((failure) => (
            <li
              key={failure.job_id}
              className="text-[11px] text-danger-foreground"
            >
              <code className="font-mono">{failure.job_id.slice(0, 8)}</code>:{" "}
              {failure.error}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
