"use client";

/**
 * Undo (UI.md section 38).
 *
 * Lists reversible actions the signed-in user took. Executing one restores the
 * recorded prior state, so a mistaken status change is one click to reverse
 * rather than a second hand-edited PATCH.
 */

import { useState } from "react";
import { Undo2 } from "lucide-react";

import { useList } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { formatRelative } from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { Button } from "@/components/ui/button";

interface Entry {
  id: string;
  action: string;
  resource_type: string;
  resource_id: string;
  created_at: string;
  expires_at: string;
}

export function UndoBar() {
  const entries = useList<Entry>("/undo");
  const toast = useToast();
  const [busy, setBusy] = useState<string | null>(null);

  if (entries.state !== "ready" || entries.rows.length === 0) return null;

  async function undoOne(entry: Entry) {
    setBusy(entry.id);
    try {
      const result = await api.post<{ restored_status?: string }>(
        `/undo/${entry.id}`,
        {},
      );
      // The description carries the detail; the second argument is an action,
      // not a message.
      toast.success(
        result.restored_status
          ? `Reverted to ${result.restored_status.toLowerCase()}`
          : "Change reverted",
      );
      entries.reload();
    } catch (error) {
      toast.error(
        "Could not revert",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(null);
    }
  }

  const latest = entries.rows[0];

  return (
    <div className="flex items-center gap-2 rounded-lg border border-border px-3 py-2 text-xs">
      <Undo2 className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
      <span className="min-w-0 flex-1 truncate">
        {latest.action.replace(/_/g, " ").toLowerCase()} on{" "}
        {latest.resource_type.toLowerCase()}{" "}
        <code className="text-[11px]">{latest.resource_id.slice(0, 8)}</code>{" "}
        {formatRelative(latest.created_at)}
      </span>
      <Button
        variant="outline"
        size="sm"
        disabled={busy === latest.id}
        onClick={() => undoOne(latest)}
      >
        {busy === latest.id ? "Reverting…" : "Undo"}
      </Button>
    </div>
  );
}
