"use client";

/**
 * Emergency controls (UI.md section 72).
 *
 * One place to see what is stopped and to stop more. Cancelling running work
 * requires a reason and an explicit confirmation, because it is destructive.
 */

import { useState } from "react";
import { Octagon, ShieldAlert } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { formatRelative } from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { Badge } from "@/components/ui/badge";
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
import { MaintenanceControl } from "@/components/ui/maintenance-control";

interface Emergency {
  maintenance: { reason: string; started_at: string } | null;
  queues: { id: string; name: string; paused: boolean; depth: number }[];
  running_executions: number;
  paused_schedules: number;
}

export default function EmergencyPage() {
  const state = useQuery<Emergency>("/emergency");
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);

  async function cancelRunning() {
    if (reason.trim() === "") {
      toast.error("A reason is required");
      return;
    }
    setBusy(true);
    try {
      const result = await api.post<{ cancelled: number }>(
        "/emergency/cancel-running",
        { reason },
      );
      toast.success(
        `${result.cancelled} execution${result.cancelled === 1 ? "" : "s"} cancelled`,
      );
      setOpen(false);
      setReason("");
      state.reload();
    } catch (error) {
      toast.error(
        "Could not cancel",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-4 p-6">
      <header>
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <ShieldAlert className="size-4" aria-hidden />
          Emergency controls
        </h1>
        <p className="text-xs text-muted-foreground">
          Everything currently stopped, and the stops you can apply.
        </p>
      </header>

      <AsyncBoundary
        state={state.state}
        error={state.error}
        forbidden={state.forbidden}
        empty={false}
        onRetry={state.reload}
        loadingLabel="Reading emergency state"
      >
        {state.data ? (
          <div className="flex flex-col gap-4">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Running</CardTitle>
                </CardHeader>
                <CardContent className="text-2xl font-semibold tabular-nums">
                  {state.data.running_executions}
                </CardContent>
              </Card>
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Paused queues</CardTitle>
                </CardHeader>
                <CardContent className="text-2xl font-semibold tabular-nums">
                  {state.data.queues.filter((q) => q.paused).length}
                  <span className="ml-1 text-sm text-muted-foreground">
                    of {state.data.queues.length}
                  </span>
                </CardContent>
              </Card>
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Paused schedules</CardTitle>
                </CardHeader>
                <CardContent className="text-2xl font-semibold tabular-nums">
                  {state.data.paused_schedules}
                </CardContent>
              </Card>
            </div>

            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Stop everything running</CardTitle>
              </CardHeader>
              <CardContent className="flex flex-col gap-2">
                <p className="text-xs text-muted-foreground">
                  Cancels every execution that is dispatched or running. Queued
                  work is not affected and will still run afterwards.
                </p>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={state.data.running_executions === 0}
                  onClick={() => setOpen(true)}
                  className="self-start"
                >
                  <Octagon className="mr-1 size-3.5" aria-hidden />
                  Cancel running executions
                </Button>
              </CardContent>
            </Card>

            <MaintenanceControl />
          </div>
        ) : null}
      </AsyncBoundary>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Cancel running executions</DialogTitle>
            <DialogDescription>
              This stops work that is already in flight. It cannot be undone, so
              a reason is required and recorded.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-1">
            <Label htmlFor="cancel-reason">Reason</Label>
            <Input
              id="cancel-reason"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder="Deploying a breaking change"
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>
              Keep running
            </Button>
            <Button onClick={cancelRunning} disabled={busy}>
              {busy ? "Cancelling…" : "Cancel everything running"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
