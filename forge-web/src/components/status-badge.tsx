"use client";

/**
 * Status and priority badges.
 *
 * Three rules hold across all of them:
 *
 * 1. Colour is never the only signal. A badge pairs its colour with a word, and
 *    the states most likely to be missed while scanning carry a glyph too. A
 *    reader who cannot distinguish red from grey still reads "Dead lettered".
 * 2. The mapping is an explicit table rather than something derived from the
 *    string, so a state the console does not know renders neutrally instead of
 *    being guessed at.
 * 3. A machine token is never clipped. `DEPENDENCY_UNAVAILABLE` truncated to
 *    `DEPENDENCY_UNAVAILABL` hides the exact value an operator needs in order to
 *    decide whether a retry is safe, so error classes wrap instead of truncating.
 */

import { AlertCircle, CheckCircle2 } from "lucide-react";

import { STATUS_LABELS } from "@/lib/types";
import { cn } from "cn";

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
  // An unrecognised state shows as itself rather than as a plausible guess, and
  // `status` may be absent, so it is read defensively.
  const label =
    STATUS_LABELS[status] ??
    String(status ?? "unknown")
      .replace(/_/g, " ")
      .toLowerCase();

  return (
    <span
      className={cn(
        "inline-flex max-w-full items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium",
        tone,
        className,
      )}
    >
      <Glyph status={status} />
      <span className="truncate">{label}</span>
    </span>
  );
}

/** A glyph only where colour alone would be easy to miss. */
function Glyph({ status }: { status: string }) {
  if (status === "SUCCEEDED" || status === "ACTIVE" || status === "READY") {
    return <CheckCircle2 className="size-3 shrink-0" aria-hidden />;
  }
  if (
    status === "FAILED" ||
    status === "TIMED_OUT" ||
    status === "DEAD_LETTERED" ||
    status === "ABANDONED"
  ) {
    return <AlertCircle className="size-3 shrink-0" aria-hidden />;
  }
  return null;
}

/**
 * An error class such as `DEPENDENCY_UNAVAILABLE`.
 *
 * These tokens are long and they are the input to the retry decision, so this
 * badge uses a square monospace face and `break-all`: wrapping onto a second
 * line is acceptable, silently dropping the last characters is not. Rendered
 * nowhere when there is no error class, because an empty error pill reads as a
 * state the operator has to interpret.
 */
export function ErrorClassBadge({
  errorClass,
  className,
}: {
  errorClass: string | null | undefined;
  className?: string;
}) {
  if (!errorClass) return null;
  return (
    <span
      className={cn(
        "inline-flex max-w-full items-center rounded bg-red-500/15 px-1.5 py-0.5",
        "font-mono text-[10px] leading-4 tracking-tight break-all",
        "text-red-600 dark:text-red-400",
        className,
      )}
      title={errorClass}
    >
      {errorClass}
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
