"use client";

/**
 * The command palette (UI.md section 4), opened with Cmd/Ctrl-K.
 *
 * Built on shadcn's `CommandDialog`, so it traps focus, closes on Escape, and
 * supports type-ahead. Two groups: actions, then navigation. When a job page is
 * open the palette also offers the context actions for *that* job, because
 * "Run this job" is meaningless anywhere else.
 *
 * Items navigate through the router in `onSelect` rather than wrapping a link:
 * cmdk owns selection and keyboard focus inside the dialog, and a nested anchor
 * would fight it.
 */

import { usePathname, useRouter } from "next/navigation";
import {
  Bell,
  CircleDot,
  Copy,
  FileText,
  Layers,
  Pause,
  Pencil,
  Play,
  Plus,
  Search,
  Server,
  SquareActivity,
  Users,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from "@/components/ui/command";
import { buildPaletteNav } from "@/components/layout/nav";

interface Command {
  label: string;
  href: string;
  icon: LucideIcon;
}

export function CommandPaletteOverlay({ onClose }: { onClose: () => void }) {
  const pathname = usePathname();
  const router = useRouter();

  const go = (href: string) => {
    onClose();
    router.push(href);
  };

  // Context-aware commands: only offered while viewing a job, because acting on
  // "this job" is meaningless anywhere else.
  const jobId = pathname.match(/^\/jobs\/([0-9a-f-]{36})$/)?.[1];

  const jobActions: Command[] = jobId
    ? [
        { label: "Run this job now", href: `/jobs/${jobId}`, icon: Play },
        { label: "Pause this job", href: `/jobs/${jobId}`, icon: Pause },
        { label: "Edit this job", href: `/jobs/${jobId}`, icon: Pencil },
        { label: "Copy this job id", href: `/jobs/${jobId}`, icon: Copy },
        {
          label: "View this job's executions",
          href: `/executions?job_id=${jobId}`,
          icon: FileText,
        },
      ]
    : [];

  const actions: Command[] = [
    { label: "Create job", href: "/jobs/builder", icon: Plus },
    { label: "Run a job", href: "/jobs", icon: Play },
    { label: "Search jobs", href: "/jobs", icon: Search },
    {
      label: "Open failed executions",
      href: "/executions?status=FAILED",
      icon: SquareActivity,
    },
    { label: "View running now", href: "/running", icon: CircleDot },
    { label: "View alerts", href: "/alerts", icon: Bell },
    { label: "View workers", href: "/workers", icon: Server },
    { label: "View queues", href: "/queues", icon: Layers },
    { label: "View system health", href: "/system-health", icon: SquareActivity },
  ];

  return (
    <CommandDialog
      open
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
      title="Command palette"
      description="Jump to a screen or run an action"
    >
      <CommandInput placeholder="Search commands and screens…" />
      <CommandList>
        <CommandEmpty>No matching command.</CommandEmpty>

        {jobActions.length > 0 ? (
          <CommandGroup heading="This job">
            {jobActions.map((command) => (
              <CommandItem
                key={command.label}
                value={command.label}
                onSelect={() => go(command.href)}
              >
                <command.icon
                  className="size-3.5 text-muted-foreground"
                  aria-hidden
                />
                <span>{command.label}</span>
              </CommandItem>
            ))}
          </CommandGroup>
        ) : null}

        <CommandGroup heading="Actions">
          {actions.map((command) => (
            <CommandItem
              key={command.label}
              value={command.label}
              onSelect={() => go(command.href)}
            >
              <command.icon
                className="size-3.5 text-muted-foreground"
                aria-hidden
              />
              <span>{command.label}</span>
            </CommandItem>
          ))}
        </CommandGroup>

        <CommandSeparator />

        <CommandGroup heading="Go to">
          {buildPaletteNav().map((item) => (
            <CommandItem
              key={item.href}
              value={`go to ${item.label}`}
              onSelect={() => go(item.href)}
            >
              <item.icon
                className="size-3.5 text-muted-foreground"
                aria-hidden
              />
              <span>{item.label}</span>
            </CommandItem>
          ))}
          <CommandItem value="go to teams" onSelect={() => go("/teams")}>
            <Users className="size-3.5 text-muted-foreground" aria-hidden />
            <span>Teams</span>
          </CommandItem>
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}