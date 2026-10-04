"use client";

/**
 * Global navigation (UI.md section 1).
 *
 * The navigation is grouped by work pattern — Monitor, Define, Capacity,
 * Govern — rather than listed flat. A flat list of twenty destinations gives no
 * hint about how the product is organised, and it left four clock icons
 * ("Schedules", "Calendar", "Running now", "Upcoming") competing unlabelled.
 * Grouping teaches the model: watch what is happening, define the work, size
 * the capacity, then govern it.
 *
 * Counts carry the same status colour used everywhere else in the console, so a
 * red badge beside Alerts is the same red as a failed execution.
 */

import type { LucideIcon } from "lucide-react";
import {
  Activity,
  Bell,
  CalendarClock,
  CalendarDays,
  CircleDot,
  ClipboardList,
  Layers,
  Plug,
  ScrollText,
  Settings,
  ShieldAlert,
  Workflow,
  Users,
} from "lucide-react";

export interface NavItem {
  href: string;
  label: string;
  icon: LucideIcon;
  /** Rendered as a badge; omitted when the count is unknown. */
  count?: number;
  /** Status tone for the count, so it agrees with the resource's own colour. */
  tone?: "bad" | "wait";
}

export interface NavGroup {
  label: string;
  items: NavItem[];
}

/** The live counts behind the sidebar badges. */
export interface NavCounts {
  running?: number;
  alerts?: number;
  incidents?: number;
  jobs?: number;
  workflows?: number;
  queues?: number;
  workers?: number;
}

export function buildNav(counts: NavCounts): NavGroup[] {
  return [
    {
      label: "Monitor",
      items: [
        { href: "/", label: "Dashboard", icon: Activity },
        { href: "/running", label: "Running now", icon: CircleDot, count: counts.running },
        { href: "/upcoming", label: "Upcoming", icon: CalendarClock },
        { href: "/alerts", label: "Alerts", icon: Bell, count: counts.alerts, tone: "bad" },
        {
          href: "/incidents",
          label: "Incidents",
          icon: ShieldAlert,
          count: counts.incidents,
          tone: "wait",
        },
      ],
    },
    {
      label: "Define",
      items: [
        { href: "/jobs", label: "Jobs", icon: ClipboardList, count: counts.jobs },
        { href: "/schedules", label: "Schedules", icon: CalendarClock },
        { href: "/workflows", label: "Workflows", icon: Workflow, count: counts.workflows },
        { href: "/calendar", label: "Calendar", icon: CalendarDays },
      ],
    },
    {
      label: "Capacity",
      items: [
        { href: "/queues", label: "Queues", icon: Layers, count: counts.queues },
        { href: "/workers", label: "Workers", icon: Users, count: counts.workers },
      ],
    },
    {
      label: "Govern",
      items: [
        { href: "/audit", label: "Audit log", icon: ScrollText },
        { href: "/integrations", label: "Integrations", icon: Plug },
        { href: "/admin", label: "Administration", icon: Settings },
      ],
    },
  ];
}

/** Destinations offered by the command palette, which shows no groups. */
export function buildPaletteNav(): NavItem[] {
  return buildNav({}).flatMap((group) => group.items);
}