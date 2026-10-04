"use client";

/**
 * Real-time connection state (UI.md section 57).
 *
 * Shows `● Live` while the console is polling successfully and a distinct
 * retrying state when it is not. Crucially it never presents stale data as
 * current: when disconnected, the label says so.
 */

import { useEffect, useState } from "react";
import { PauseCircle, Radio } from "lucide-react";

import { API_URL } from "@/lib/api";
import { cn } from "cn";

export function LiveIndicator({
  paused,
  onToggle,
}: {
  paused: boolean;
  onToggle: (next: boolean) => void;
}) {
  const [state, setState] = useState<"live" | "retrying">("live");

  // Poll the liveness endpoint; three consecutive failures means "retrying".
  useEffect(() => {
    let cancelled = false;
    let failures = 0;

    const check = async () => {
      try {
        // The real liveness endpoint. It needs no token, so this works even
        // when the session has expired.
        const response = await fetch(`${API_URL}/health/live`);
        if (!response.ok) throw new Error("unreachable");
        failures = 0;
        if (!cancelled) setState("live");
      } catch {
        failures += 1;
        if (!cancelled && failures >= 3) setState("retrying");
      }
    };

    check();
    const timer = setInterval(check, 15000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  const label = paused
    ? "Paused"
    : state === "live"
      ? "Live"
      : "Live updates disconnected — retrying…";

  return (
    <button
      type="button"
      onClick={() => onToggle(!paused)}
      title={paused ? "Resume auto-refresh" : "Pause auto-refresh"}
      aria-label={label}
      className={cn(
        "flex items-center gap-1 rounded border border-border px-1.5 py-0.5 text-[10px]",
        paused
          ? "text-muted-foreground"
          : state === "live"
            ? "text-emerald-600 dark:text-emerald-400"
            : "text-amber-700 dark:text-amber-400",
      )}
    >
      {paused ? (
        <PauseCircle className="size-3" aria-hidden />
      ) : (
        <Radio className="size-3" aria-hidden />
      )}
      <span aria-live="polite">
        {paused ? "paused" : state === "live" ? "Live" : "retrying"}
      </span>
    </button>
  );
}
