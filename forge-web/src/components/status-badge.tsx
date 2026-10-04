"use client";

/**
 * Status and priority indicators.
 *
 * One table maps every state the console knows to a tone, a glyph, and a word,
 * so a status reads the same everywhere it appears and no page invents its own
 * colour for "failed".
 *
 * Three rules hold across all of them:
 *
 * 1. Colour is never the only signal (UI.md §6). Each indicator pairs its
 *    colour with a glyph and a word, so an operator who cannot separate red
 *    from grey still reads "Dead lettered" and sees a distinct shape. This is
 *    also what makes a row scannable at 40 rows.
 * 2. The mapping is an explicit table rather than something derived from the
 *    string, so a state the console does not know renders neutrally instead of
 *    being guessed at.
 * 3. A machine token is never clipped. `DEPENDENCY_UNAVAILABLE` truncated to
 *    `DEPENDENCY_UNAVAILABL` hides the exact value an operator needs to decide
 *    whether a retry is safe, so error classes wrap instead of truncating.
 */

import {
  AlertTriangle,
  Ban,
  Check,
  CircleDot,
  Clock,
  Hourglass,
  PauseCircle,
  PlayCircle,
  XCircle,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

import { STATUS_LABELS } from "@/lib/types";
import { cn } from "cn";

/**
 * The five tones the console spends. Deliberately few: a status vocabulary
 * that needs a sixth colour stops being readable.
 *
 * - `neutral`   settled, not interesting
 * - `info`      in progress, healthy but moving
 * - `success`   finished correctly
 * - `warning`   waiting, or attention soon
 * - `danger`    failed; something needs a human
 */
export type Tone = "neutral" | "info" | "success" | "warning" | "danger";

/** Dot + text on a tinted ground. The table-cell default. */
const DOT_TONE: Record<Tone, string> = {
  neutral: "bg-muted-foreground/45",
  info: "bg-info",
  success: "bg-success",
  warning: "bg-warning",
  danger: "bg-danger",
};

/** Solid chip. Reserved for detail pages and toolbars, where it has room. */
const CHIP_TONE: Record<Tone, string> = {
  neutral: "bg-muted text-muted-foreground",
  info: "bg-info/12 text-info-foreground",
  success: "bg-success/12 text-success-foreground",
  warning: "bg-warning/15 text-warning-foreground",
  danger: "bg-danger/12 text-danger-foreground",
};

/**
 * Status → tone + glyph, keyed by the exact enum variants the API sends.
 *
 * An unknown key falls through to `neutral` and renders its own label, so a
 * new backend state appears as itself rather than as a plausible guess.
 */
const STATUS_TONE: Record<string, { tone: Tone; glyph: LucideIcon }> = {
  // Settled and healthy.
  SUCCEEDED: { tone: "success", glyph: Check },
  ACTIVE: { tone: "success", glyph: PlayCircle },
  READY: { tone: "success", glyph: Check },

  // In flight.
  RUNNING: { tone: "info", glyph: CircleDot },
  DISPATCHED: { tone: "info", glyph: CircleDot },
  BUSY: { tone: "info", glyph: CircleDot },
  REGISTERING: { tone: "info", glyph: Hourglass },

  // Waiting for something.
  QUEUED: { tone: "warning", glyph: Hourglass },
  SCHEDULED: { tone: "neutral", glyph: Clock },
  RETRY_SCHEDULED: { tone: "warning", glyph: Clock },
  CANCEL_REQUESTED: { tone: "warning", glyph: PauseCircle },
  DRAINING: { tone: "warning", glyph: PauseCircle },
  PENDING: { tone: "neutral", glyph: Clock },

  // Failed.
  FAILED: { tone: "danger", glyph: XCircle },
  TIMED_OUT: { tone: "danger", glyph: Clock },
  DEAD_LETTERED: { tone: "danger", glyph: Ban },
  ABANDONED: { tone: "danger", glyph: Ban },
  DENIED: { tone: "danger", glyph: Ban },
  FAILURE: { tone: "danger", glyph: XCircle },

  // Settled without incident.
  CANCELLED: { tone: "neutral", glyph: Ban },
  ARCHIVED: { tone: "neutral", glyph: Ban },
  DRAFT: { tone: "neutral", glyph: CircleDot },
  OFFLINE: { tone: "neutral", glyph: XCircle },
  REVOKED: { tone: "neutral", glyph: Ban },
  SUCCESS: { tone: "success", glyph: Check },
};

/** Resolves a raw status string to its tone and glyph. */
export function statusTone(status: string | null | undefined): {
  tone: Tone;
  glyph: LucideIcon;
  label: string;
} {
  const key = String(status ?? "").toUpperCase();
  const hit = STATUS_TONE[key];
  const label =
    STATUS_LABELS[key] ??
    key
      .replace(/_/g, " ")
      .toLowerCase()
      .replace(/^./, (c) => c.toUpperCase());
  return {
    tone: hit?.tone ?? "neutral",
    glyph: hit?.glyph ?? CircleDot,
    // An absent status is shown as such rather than as an empty pill, which
    // would read as a state the operator has to interpret.
    label: status ? label : "Unknown",
  };
}

/**
 * The table-cell status indicator: a dot plus the word.
 *
 * The dot is `aria-hidden` because the word beside it already carries the
 * state — announcing "green dot" on top of "Failed" helps nobody.
 *
 * A `running` dot pulses so "what is executing right now" is answerable by a
 * squint, not by reading 40 rows.
 */
export function StatusCell({
  status,
  className,
}: {
  status: string | null | undefined;
  className?: string;
}) {
  const { tone, label } = statusTone(status);
  const live = status === "RUNNING" || status === "BUSY" || status === "DISPATCHED";

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 whitespace-nowrap text-[12.5px]",
        tone === "danger" && "text-danger-foreground",
        tone === "warning" && "text-warning-foreground",
        className,
      )}
    >
      <span
        aria-hidden
        className={cn(
          "size-1.5 shrink-0 rounded-full",
          DOT_TONE[tone],
          live && "forge-pulse",
        )}
      />
      <span className={tone === "neutral" ? "text-muted-foreground" : undefined}>
        {label}
      </span>
    </span>
  );
}

/**
 * A solid status chip, for detail headers and toolbars where there is room for
 * the whole word.
 */
export function StatusBadge({
  status,
  className,
}: {
  status: string | null | undefined;
  className?: string;
}) {
  const { tone, label } = statusTone(status);

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded px-1.5 py-0.5 text-[11px] font-medium whitespace-nowrap",
        CHIP_TONE[tone],
        className,
      )}
    >
      <span
        aria-hidden
        className={cn("size-1.5 shrink-0 rounded-full", DOT_TONE[tone])}
      />
      {label}
    </span>
  );
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
        "inline-flex max-w-full items-center rounded bg-danger/12 px-1.5 py-0.5",
        "font-mono text-[10.5px] leading-4 tracking-tight break-all",
        "text-danger-foreground",
        className,
      )}
      title={errorClass}
    >
      {errorClass}
    </span>
  );
}

const PRIORITY_TONE: Record<string, Tone> = {
  CRITICAL: "danger",
  HIGH: "warning",
  NORMAL: "neutral",
  LOW: "neutral",
  BACKGROUND: "neutral",
};

/**
 * Priority, written as a short lowercase word.
 *
 * Only CRITICAL and HIGH are tinted. Tinting all five levels would spend the
 * whole status vocabulary on a field that is usually unremarkable, and the
 * levels that matter would stop standing out.
 */
export function PriorityBadge({
  priority,
  className,
}: {
  priority: string | null | undefined;
  className?: string;
}) {
  if (!priority) return null;
  const key = priority.toUpperCase();
  const tone = PRIORITY_TONE[key] ?? "neutral";
  const label = key.charAt(0) + key.slice(1).toLowerCase();

  if (tone === "neutral") {
    return (
      <span className={cn("text-[12.5px] text-muted-foreground", className)}>
        {label}
      </span>
    );
  }

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 whitespace-nowrap text-[12.5px]",
        tone === "danger" ? "text-danger-foreground" : "text-warning-foreground",
        className,
      )}
    >
      <span
        aria-hidden
        className={cn(
          "size-1.5 shrink-0 rounded-[2px]",
          tone === "danger" ? "bg-danger" : "bg-warning",
        )}
      />
      {label}
    </span>
  );
}

/**
 * A named resource id, shown in full and copyable.
 *
 * The first 8 characters are the readable part but the *whole* id is the lookup
 * key, so the full value is the tooltip and the copy payload. Truncating what
 * gets copied is how "copy id" hands back something that cannot be pasted into
 * the next command.
 */
export function ResourceId({
  id,
  className,
  label = "ID",
}: {
  id: string | null | undefined;
  className?: string;
  label?: string;
}) {
  if (!id) return <span className="text-muted-foreground">-</span>;
  return (
    <span className={cn("inline-flex items-center gap-1", className)}>
      <code className="font-mono text-[11.5px] text-muted-foreground" title={id}>
        {id.slice(0, 8)}
      </code>
      <CopyHint value={id} label={`Copy ${label}`} />
    </span>
  );
}

/** A small copy-to-clipboard affordance with an accessible name. */
export function CopyHint({
  value,
  label = "Copy",
  className,
}: {
  value: string;
  label?: string;
  className?: string;
}) {
  return (
    <button
      type="button"
      onClick={() => {
        void navigator.clipboard?.writeText(value);
      }}
      aria-label={label}
      title={label}
      className={cn(
        "grid size-4 shrink-0 place-items-center rounded text-muted-foreground/60",
        "transition-colors hover:bg-accent hover:text-foreground",
        className,
      )}
    >
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
        className="size-2.5"
      >
        <rect x="9" y="9" width="12" height="12" rx="2" />
        <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
      </svg>
    </button>
  );
}

/** A compact count pill used in list rows and detail panels. */
export function CountPill({
  value,
  tone = "neutral",
  className,
}: {
  value: number | null | undefined;
  tone?: Tone;
  className?: string;
}) {
  if (value === null || value === undefined) return null;
  return (
    <span
      className={cn(
        "inline-flex min-w-[1.25rem] items-center justify-center rounded px-1 py-px",
        "text-[11px] font-medium tabular-nums",
        CHIP_TONE[tone],
        className,
      )}
    >
      {value}
    </span>
  );
}

/**
 * A severity marker for alert-like lists: a small dot plus an uppercase word.
 *
 * Quiet by design. A full coloured card per alert turns an inbox into a
 * billboard; the dot is enough when the word is already beside it.
 */
export function SeverityTag({
  severity,
  className,
}: {
  severity: string | null | undefined;
  className?: string;
}) {
  const key = String(severity ?? "").toUpperCase();
  const tone: Tone =
    key === "CRITICAL" || key === "FATAL"
      ? "danger"
      : key === "WARNING" || key === "WARN"
        ? "warning"
        : key === "INFO"
          ? "info"
          : "neutral";

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 text-[11px] font-medium tracking-wide uppercase",
        tone === "danger" && "text-danger-foreground",
        tone === "warning" && "text-warning-foreground",
        tone === "info" && "text-info-foreground",
        tone === "neutral" && "text-muted-foreground",
        className,
      )}
    >
      <span aria-hidden className={cn("size-1.5 rounded-full", DOT_TONE[tone])} />
      {key || "unknown"}
    </span>
  );
}

/**
 * A warning icon with an accessible name, for use beside a word.
 *
 * Kept because "attention needed" is the one signal where a bare dot is not
 * enough context on its own.
 */
export function AlertGlyph({ className }: { className?: string }) {
  return (
    <AlertTriangle className={cn("size-3.5 shrink-0", className)} aria-hidden />
  );
}