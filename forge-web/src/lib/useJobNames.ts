"use client";

/**
 * Maps job ids to their names, for screens that hold executions.
 *
 * `GET /executions` returns a `job_id` but no job name, so an execution table
 * that shows ids alone makes the operator look each run up by hand. There is no
 * endpoint that hydrates names server-side, and there does not need to be: the
 * job list is small, already paginated to a page, and cached for the session.
 *
 * Two rules:
 *
 * - A name is shown only when it was actually resolved. An execution whose job
 *   was deleted, or whose id is not in the loaded page, renders the id — never
 *   a blank cell and never a guessed name.
 * - The map is keyed by id, so it stays correct when two jobs share a name.
 */

import { useMemo } from "react";

import { useList } from "@/lib/useQuery";
import type { Job } from "@/lib/types";

export function useJobNames(): {
  nameFor: (jobId: string | null | undefined) => string | null;
  keyFor: (jobId: string | null | undefined) => string | null;
} {
  // A generous page: the console routinely shows a hundred executions drawn
  // from fewer distinct jobs, and a partial map silently degrades to ids.
  const jobs = useList<Job>("/jobs?limit=200");

  const byId = useMemo(() => {
    const map = new Map<string, Job>();
    for (const job of jobs.rows) map.set(job.id, job);
    return map;
  }, [jobs.rows]);

  return useMemo(
    () => ({
      nameFor: (jobId) => (jobId ? (byId.get(jobId)?.name ?? null) : null),
      keyFor: (jobId) => (jobId ? (byId.get(jobId)?.key ?? null) : null),
    }),
    [byId],
  );
}