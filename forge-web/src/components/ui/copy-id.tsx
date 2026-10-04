"use client";

/**
 * Copy an identifier to the clipboard (UI.md section 68).
 *
 * Every resource id is copyable, and the button says so on failure rather than
 * silently doing nothing.
 */

import { Check, Copy, X } from "lucide-react";

import { useCopy } from "@/lib/clipboard";
import { cn } from "cn";

export function CopyId({
  value,
  label = "Copy ID",
  className,
}: {
  value: string;
  label?: string;
  className?: string;
}) {
  const { copy, copied, failed } = useCopy();

  return (
    <button
      type="button"
      onClick={() => copy(value)}
      aria-label={label}
      className={cn(
        "inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-accent/60 hover:text-foreground",
        failed && "text-red-600",
        className,
      )}
    >
      {copied ? (
        <Check className="size-3" aria-hidden />
      ) : failed ? (
        <X className="size-3" aria-hidden />
      ) : (
        <Copy className="size-3" aria-hidden />
      )}
      <span aria-live="polite">
        {copied ? "Copied" : failed ? "Copy failed" : label}
      </span>
    </button>
  );
}
