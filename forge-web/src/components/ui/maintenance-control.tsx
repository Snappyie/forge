"use client";

/**
 * Maintenance mode (UI.md section 71).
 *
 * Entering maintenance holds scheduling for the whole tenant, so it requires a
 * reason and an explicit confirmation. The control shows the live state rather
 * than a toggle that might be stale.
 */

import { useState } from "react";
import { PauseCircle, PlayCircle } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { formatRelative, formatTimestamp } from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface Maintenance {
  active: boolean;
  window: {
    id: string;
    reason: string;
    started_at: string;
    started_by: string | null;
  } | null;
}

export function MaintenanceControl() {
  const state = useQuery<Maintenance>("/maintenance");
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);

  const active = state.data?.active ?? false;
  const window = state.data?.window ?? null;

  async function start() {
    if (reason.trim() === "") {
      toast.error("A reason is required", "Explain why scheduling is being held.");
      return;
    }
    setBusy(true);
    try {
      await api.post("/maintenance", { reason });
      toast.success("Maintenance mode enabled");
      setOpen(false);
      setReason("");
      state.reload();
    } catch (error) {
      toast.error(
        "Could not enable maintenance",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  async function end() {
    setBusy(true);
    try {
      await api.delete("/maintenance");
      toast.success("Maintenance mode lifted");
      state.reload();
    } catch (error) {
      toast.error(
        "Could not lift maintenance",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card>
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle className="text-sm">Maintenance mode</CardTitle>
        {active ? (
          <span className="rounded border border-amber-500/50 bg-amber-500/10 px-1.5 py-0.5 text-[10px] uppercase text-amber-700 dark:text-amber-400">
            active
          </span>
        ) : (
          <span className="rounded border border-border px-1.5 py-0.5 text-[10px] uppercase text-muted-foreground">
            off
          </span>
        )}
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {active && window ? (
          <div className="rounded-md border border-amber-500/40 bg-amber-500/5 p-3 text-xs">
            <p className="font-medium">{window.reason}</p>
            <p className="mt-0.5 text-[11px] text-muted-foreground">
              started {formatRelative(window.started_at)} ·{" "}
              {formatTimestamp(window.started_at)}
            </p>
          </div>
        ) : (
          <p className="text-xs text-muted-foreground">
            Scheduling runs normally. Enabling maintenance holds new executions
            for this tenant until it is lifted.
          </p>
        )}

        {active ? (
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            onClick={end}
            className="self-start"
          >
            <PlayCircle className="mr-1 size-3.5" aria-hidden />
            {busy ? "Lifting…" : "Lift maintenance"}
          </Button>
        ) : (
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            onClick={() => setOpen(true)}
            className="self-start"
          >
            <PauseCircle className="mr-1 size-3.5" aria-hidden />
            Enable maintenance
          </Button>
        )}
      </CardContent>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Enable maintenance mode</DialogTitle>
            <DialogDescription>
              No new executions will be dispatched while maintenance is on. Runs
              already in flight are not interrupted.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-1">
            <Label htmlFor="maintenance-reason">Reason</Label>
            <Input
              id="maintenance-reason"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="Upgrading the scheduler fleet"
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button onClick={start} disabled={busy}>
              {busy ? "Enabling…" : "Enable maintenance"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
