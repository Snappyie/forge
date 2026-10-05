"use client";

/**
 * Global navigation (UI.md section 1).
 *
 * Grouped by the work the operator is doing rather than by route: Monitor
 * (what is happening), Define (what should happen), Capacity (where it runs),
 * Govern (who and why). A flat list of twenty destinations teaches nothing and
 * leaves four clock icons competing unlabelled.
 *
 * Counts come from the dashboard aggregate the shell already requests, so a
 * badge can never disagree with the number behind it. An unknown count is
 * omitted rather than shown as zero — "0 alerts" is a confident wrong answer
 * when the request simply failed.
 */

import type { LucideIcon } from "lucide-react";
import {
  Activity,
  Bell,
  Boxes,
  GitCompareArrows,
  Globe,
  CalendarClock,
  CalendarDays,
  CircleDot,
  ClipboardList,
  Layers,
  Plug,
  ScrollText,
  Settings,
  ShieldAlert,
  Users,
  Workflow,
} from "lucide-react";

export interface NavItem {
  href: string;
  label: string;
  icon: LucideIcon;
  /** Rendered as a badge; omitted when the count is unknown. */
  count?: number;
  /** Status tone, so the badge agrees with the resource's own colour. */
  tone?: "danger" | "warning";
}

export interface NavGroup {
  label: string;
  items: NavItem[];
}

/** The live counts behind the sidebar badges. */
export interface NavCounts {
  applications?: number;
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
        {
          href: "/running",
          label: "Running now",
          icon: CircleDot,
          count: counts.running,
          tone: "danger",
        },
        { href: "/upcoming", label: "Upcoming", icon: CalendarClock },
        {
          href: "/alerts",
          label: "Alerts",
          icon: Bell,
          count: counts.alerts,
          tone: "danger",
        },
        {
          href: "/incidents",
          label: "Incidents",
          icon: ShieldAlert,
          count: counts.incidents,
          tone: "warning",
        },
      ],
    },
    {
      label: "Define",
      items: [
        {
          href: "/applications",
          label: "Applications",
          icon: Boxes,
          count: counts.applications,
        },
        { href: "/jobs", label: "Jobs", icon: ClipboardList, count: counts.jobs },
        { href: "/schedules", label: "Schedules", icon: CalendarClock },
        {
          href: "/workflows",
          label: "Workflows",
          icon: Workflow,
          count: counts.workflows,
        },
        { href: "/calendar", label: "Calendar", icon: CalendarDays },
      ],
    },
    {
      label: "Capacity",
      items: [
        { href: "/environments", label: "Environments", icon: Globe },
        { href: "/queues", label: "Queues", icon: Layers, count: counts.queues },
        { href: "/workers", label: "Workers", icon: Users, count: counts.workers },
      ],
    },
    {
      label: "Govern",
      items: [
        { href: "/migration", label: "Migrate jobs", icon: GitCompareArrows },
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