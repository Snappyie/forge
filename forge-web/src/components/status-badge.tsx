"use client";

/**
 * A status badge with consistent colour per state.
 *
 * The mapping is explicit rather than derived from the string, so an unknown
 * status renders neutrally instead of being guessed at.
 */

import { STATUS_LABELS } from "@/lib/types";

const TONES: Record<string, string> = {
  // Succeeded family.
  SUCCEEDED: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
  ACTIVE: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
  READY: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
  SUCCEEDED_: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",

  // In-progress family.
  RUNNING: "bg-blue-500/15 text-blue-600 dark:text-blue-400",
  DISPATCHED: "bg-blue-500/15 text-blue-600 dark:text-blue-400",
  BUSY: "bg-blue-500/15 text-blue-600 dark:text-blue-400",
  REGISTERING: "bg-blue-500/15 text-blue-600 dark:text-blue-400",

  // Waiting family.
  QUEUED: "bg-amber-500/15 text-amber-700 dark:text-amber-400",
  SCHEDULED: "bg-amber-500/15 text-amber-700 dark:text-amber-400",
  RETRY_SCHEDULED: "bg-amber-500/15 text-amber-700 dark:text-amber-400",
  CANCEL_REQUESTED: "bg-amber-500/15 text-amber-700 dark:text-amber-400",
  DRAFT: "bg-muted text-muted-foreground",
  PENDING: "bg-muted text-muted-foreground",

  // Failure family.
  FAILED: "bg-red-500/15 text-red-600 dark:text-red-400",
  TIMED_OUT: "bg-red-500/15 text-red-600 dark:text-red-400",
  DEAD_LETTERED: "bg-red-500/15 text-red-600 dark:text-red-400",
  ABANDONED: "bg-red-500/15 text-red-600 dark:text-red-400",
  REVOKED: "bg-red-500/15 text-red-600 dark:text-red-400",

  // Terminal-but-neutral family.
  CANCELLED: "bg-muted text-muted-foreground",
  ARCHIVED: "bg-muted text-muted-foreground",
  OFFLINE: "bg-muted text-muted-foreground",
  DRAINING: "bg-orange-500/15 text-orange-600 dark:text-orange-400",
};

export function StatusBadge({
  status,
  className = "",
}: {
  status: string;
  className?: string;
}) {
  const tone = TONES[status] ?? "bg-muted text-muted-foreground";
  const label = STATUS_LABELS[status] ?? status.replace(/_/g, " ").toLowerCase();

  return (
    <span
      className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${tone} ${className}`}
    >
      {label}
    </span>
  );
}

/** A priority badge; only the levels that differ from NORMAL are coloured. */
export function PriorityBadge({ priority }: { priority: string }) {
  const tones: Record<string, string> = {
    CRITICAL: "bg-red-500/15 text-red-600 dark:text-red-400",
    HIGH: "bg-orange-500/15 text-orange-600 dark:text-orange-400",
    LOW: "bg-muted text-muted-foreground",
    BACKGROUND: "bg-muted text-muted-foreground",
  };
  const tone = tones[priority] ?? "bg-muted text-muted-foreground";

  return (
    <span className={`inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${tone}`}>
      {priority.charAt(0) + priority.slice(1).toLowerCase()}
    </span>
  );
}
