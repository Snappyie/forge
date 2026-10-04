"use client";

/**
 * Dashboard customization (UI.md section 49).
 *
 * Widget visibility persists per browser. Drag/drop reordering and resizing are
 * not implemented; hiding and restoring are, and every widget here reads real
 * data, so none of them is a placeholder.
 */

import { useEffect, useState } from "react";

import { useShell } from "@/lib/shell";

const WIDGETS = [
  { key: "status", label: "Real-time status" },
  { key: "charts", label: "Execution charts" },
  { key: "attention", label: "Needs attention" },
  { key: "upcoming", label: "Upcoming" },
  { key: "queues", label: "Queues" },
] as const;

type WidgetKey = (typeof WIDGETS)[number]["key"];

const STORAGE_KEY = "forge.dashboard.widgets";

/** Reads the saved visibility map, defaulting every widget to visible. */
export function useWidgetVisibility(): [Record<string, boolean>, (key: string, on: boolean) => void] {
  const [hidden, setHidden] = useState<Record<string, boolean>>({});

  useEffect(() => {
    try {
      setHidden(JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "{}"));
    } catch {
      setHidden({});
    }
  }, []);

  function toggle(key: string, on: boolean) {
    setHidden((current) => {
      const next = { ...current, [key]: !on };
      try {
        window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      } catch {
        // A disabled store just means the choice does not persist.
      }
      return next;
    });
  }

  return [hidden, toggle];
}

export function DashboardWidgetPicker() {
  const [hidden, toggle] = useWidgetVisibility();
  const { recent } = useShell();

  return (
    <div className="flex flex-wrap items-center gap-2">
      <span className="text-[11px] uppercase tracking-wide text-muted-foreground">
        Widgets
      </span>
      {WIDGETS.map((widget) => {
        const visible = !hidden[widget.key];
        return (
          <button
            key={widget.key}
            type="button"
            onClick={() => toggle(widget.key, visible)}
            aria-pressed={visible}
            className={
              visible
                ? "rounded border border-border px-2 py-0.5 text-[11px]"
                : "rounded border border-border px-2 py-0.5 text-[11px] text-muted-foreground line-through"
            }
          >
            {widget.label}
          </button>
        );
      })}
      {/* The shell already tracks recently visited routes; surface it here so
          the personalisation surface is in one place. */}
      {recent.length === 0 ? null : (
        <span className="text-[11px] text-muted-foreground">
          · {recent.length} page{recent.length === 1 ? "" : "s"} visited recently
        </span>
      )}
    </div>
  );
}

export type { WidgetKey };
