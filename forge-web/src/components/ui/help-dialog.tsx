"use client";

/**
 * Contextual help (UI.md section 54) and the keyboard reference (section 63).
 *
 * Reachable from the header and by `?`, so help is discoverable without leaving
 * the screen an operator is on.
 */

import { useEffect, useState } from "react";
import { HelpCircle } from "lucide-react";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

const SHORTCUTS: [string, string][] = [
  ["⌘K / Ctrl+K", "Open the command palette"],
  ["/", "Focus search"],
  ["?", "Open this help"],
  ["Esc", "Close a dialog or overlay"],
  ["Enter", "Submit a simple form"],
  ["⌘Enter / Ctrl+Enter", "Save a form"],
];

export function HelpDialog() {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (
        event.key === "?" &&
        !(event.target instanceof HTMLInputElement) &&
        !(event.target instanceof HTMLTextAreaElement)
      ) {
        event.preventDefault();
        setOpen((current) => !current);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        aria-label="Help and keyboard shortcuts"
        className="shrink-0 rounded-md p-1.5 text-muted-foreground hover:bg-accent/60 hover:text-foreground"
      >
        <HelpCircle className="size-4" aria-hidden />
      </button>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Keyboard shortcuts</DialogTitle>
            <DialogDescription>
              Forge is built for keyboard use; these work on every screen.
            </DialogDescription>
          </DialogHeader>
          <dl className="flex flex-col gap-2">
            {SHORTCUTS.map(([keys, description]) => (
              <div key={keys} className="flex items-center gap-3 text-sm">
                <dt className="w-40 shrink-0">
                  <kbd className="rounded border border-border px-1.5 py-0.5 font-mono text-xs">
                    {keys}
                  </kbd>
                </dt>
                <dd className="text-muted-foreground">{description}</dd>
              </div>
            ))}
          </dl>
        </DialogContent>
      </Dialog>
    </>
  );
}
