"use client";

/**
 * Worker list (spec 7.12, UI.md section 25).
 *
 * Drain and revoke are gated on `workers:admin`, so a viewer sees the fleet
 * without seeing controls they cannot use. Revoke is irreversible, so it goes
 * through a confirmation that names the worker and states the consequence —
 * `window.confirm` cannot show what is about to happen to which worker.
 */

import { useState } from "react";
import { Loader2, Power, RefreshCw, ShieldOff } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { useList } from "@/lib/useQuery";
import {
  formatRelative,
  formatTimestamp,
  roleCan,
  type Worker,
} from "@/lib/types";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { TableBody, TableHeader, TableRow } from "@/components/ui/table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  DataTable,
  DataTableCell,
  DataTableHead,
  PageHeader,
  RowLink,
  Toolbar,
  TableFooter,
  TableSkeleton,
} from "@/components/page";
import { ResourceId, StatusCell } from "@/components/status-badge";
import { useToast } from "@/lib/useToast";
import { EmptyState, ErrorState, ForbiddenState } from "@/components/states";

const STATUSES = [
  "READY",
  "BUSY",
  "DRAINING",
  "OFFLINE",
  "REVOKED",
  "REGISTERING",
] as const;

export default function WorkersPage() {
  const { session } = useAuth();
  const [status, setStatus] = useState("ALL");
  const params = new URLSearchParams();
  params.set("limit", "100");
  if (status !== "ALL") params.set("status", status);

  const query = useList<Worker>(`/workers?${params.toString()}`);
  const rows = query.rows;
  const canAdmin = roleCan(session?.role, "workers:admin");
  const filtering = status !== "ALL";

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Workers"
        description="The fleet that claims and runs queued work."
        actions={
          <Button
            variant="outline"
            size="sm"
            onClick={query.reload}
            disabled={query.state === "loading"}
          >
            <RefreshCw
              className={query.state === "loading" ? "animate-spin" : undefined}
              aria-hidden
            />
            Refresh
          </Button>
        }
      />

      <Toolbar>
        <Select value={status} onValueChange={(value) => setStatus(value ?? "ALL")}>
          <SelectTrigger
            size="sm"
            className="h-7 w-36 text-[12.5px]"
            aria-label="Filter by status"
          >
            <SelectValue placeholder="Any status" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="ALL">Any status</SelectItem>
            {STATUSES.map((s) => (
              <SelectItem key={s} value={s}>
                {s.charAt(0) + s.slice(1).toLowerCase()}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {filtering ? (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setStatus("ALL")}
            className="h-7 text-[12.5px] text-muted-foreground"
          >
            Clear filter
          </Button>
        ) : null}
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-auto">
        {query.state === "loading" ? (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead>Worker</DataTableHead>
                <DataTableHead className="w-28">Status</DataTableHead>
                <DataTableHead className="w-32">Hostname</DataTableHead>
                <DataTableHead className="w-20">Version</DataTableHead>
                <DataTableHead className="w-32" align="right">
                  Last heartbeat
                </DataTableHead>
                {canAdmin ? (
                  <DataTableHead className="w-20" align="right">
                    Actions
                  </DataTableHead>
                ) : null}
              </TableRow>
            </TableHeader>
            <TableSkeleton rows={8} columns={canAdmin ? 6 : 5} />
          </DataTable>
        ) : query.state === "error" ? (
          query.forbidden ? (
            <ForbiddenState />
          ) : (
            <ErrorState error={query.error} onRetry={query.reload} />
          )
        ) : rows.length === 0 ? (
          <EmptyState
            title={
              filtering ? "No workers with that status" : "No workers registered"
            }
            description={
              filtering
                ? "Clear the filter to see the whole fleet."
                : "A worker registers over HTTP, claims work, holds a lease, and heartbeats. Until one connects, executions queue and nothing runs."
            }
            action={
              filtering ? (
                <Button size="sm" variant="outline" onClick={() => setStatus("ALL")}>
                  Clear filter
                </Button>
              ) : null
            }
          />
        ) : (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead>Worker</DataTableHead>
                <DataTableHead className="w-28">Status</DataTableHead>
                <DataTableHead className="w-32">Hostname</DataTableHead>
                <DataTableHead className="w-20">Version</DataTableHead>
                <DataTableHead className="w-32" align="right">
                  Last heartbeat
                </DataTableHead>
                {canAdmin ? (
                  <DataTableHead className="w-20" align="right">
                    Actions
                  </DataTableHead>
                ) : null}
              </TableRow>
            </TableHeader>

            <TableBody>
              {rows.map((worker) => (
                <TableRow key={worker.id} className="h-8">
                  <DataTableCell className="font-medium">
                    <span className="flex items-baseline gap-2">
                      <RowLink href={`/workers/${worker.id}`}>
                        {worker.name ?? worker.hostname}
                      </RowLink>
                      <ResourceId id={worker.id} label="worker id" />
                    </span>
                  </DataTableCell>

                  <DataTableCell>
                    <StatusCell status={worker.status} />
                  </DataTableCell>

                  <DataTableCell className="text-[12px] text-muted-foreground">
                    {worker.hostname}
                  </DataTableCell>

                  <DataTableCell className="text-[12px] text-muted-foreground">
                    {worker.version ?? "—"}
                  </DataTableCell>

                  <DataTableCell
                    align="right"
                    className="text-[12px] text-muted-foreground"
                    title={formatTimestamp(worker.last_heartbeat_at)}
                  >
                    {formatRelative(worker.last_heartbeat_at)}
                  </DataTableCell>

                  {canAdmin ? (
                    <DataTableCell align="right">
                      <WorkerActions worker={worker} onDone={query.reload} />
                    </DataTableCell>
                  ) : null}
                </TableRow>
              ))}
            </TableBody>
          </DataTable>
        )}
      </div>

      <TableFooter shown={rows.length} />
    </div>
  );
}

function WorkerActions({
  worker,
  onDone,
}: {
  worker: Worker;
  onDone: () => void;
}) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState<"drain" | "revoke" | null>(null);
  const label = worker.name ?? worker.hostname;

  async function act(action: "drain" | "revoke") {
    setBusy(true);
    try {
      await api.post(`/workers/${worker.id}/${action}`);
      toast.success(
        action === "drain"
          ? `${label} is draining`
          : `${label} revoked`,
      );
      setConfirming(null);
      onDone();
    } catch (cause) {
      toast.error(
        `Could not ${action} ${label}`,
        cause instanceof ApiError ? cause.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex items-center justify-end gap-0.5">
      {worker.status !== "DRAINING" && worker.status !== "REVOKED" ? (
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={busy}
          aria-label={`Drain ${label}`}
          title="Drain: finish current work, accept nothing new"
          onClick={() => setConfirming("drain")}
        >
          {busy && confirming === "drain" ? (
            <Loader2 className="animate-spin" aria-hidden />
          ) : (
            <Power aria-hidden />
          )}
        </Button>
      ) : null}

      {worker.status !== "REVOKED" ? (
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={busy}
          aria-label={`Revoke ${label}`}
          title="Revoke: permanently"
          onClick={() => setConfirming("revoke")}
        >
          {busy && confirming === "revoke" ? (
            <Loader2 className="animate-spin" aria-hidden />
          ) : (
            <ShieldOff aria-hidden />
          )}
        </Button>
      ) : null}

      <ConfirmWorkerAction
        worker={worker}
        action={confirming}
        busy={busy}
        onCancel={() => setConfirming(null)}
        onConfirm={act}
      />
    </div>
  );
}

/**
 * States the consequence before the action.
 *
 * UI.md section 62: never "are you sure", but "revoke this worker in
 * production, and here is what stops happening".
 */
function ConfirmWorkerAction({
  worker,
  action,
  busy,
  onCancel,
  onConfirm,
}: {
  worker: Worker;
  action: "drain" | "revoke" | null;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (action: "drain" | "revoke") => void;
}) {
  const label = worker.name ?? worker.hostname;
  const revoking = action === "revoke";

  return (
    <AlertDialog
      open={action !== null}
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {revoking ? `Revoke ${label}?` : `Drain ${label}?`}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {revoking ? (
              <>
                This is permanent. <strong>{label}</strong> will never be
                offered work again, and any lease it holds is abandoned. Work
                already dispatched to it fails.
              </>
            ) : (
              <>
                <strong>{label}</strong> stops accepting new work. Anything it
                has already claimed runs to completion.
              </>
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogAction variant="outline" onClick={onCancel}>
            Cancel
          </AlertDialogAction>
          <AlertDialogAction
            variant={revoking ? "destructive" : "default"}
            disabled={busy}
            onClick={() => onConfirm(revoking ? "revoke" : "drain")}
          >
            {busy ? "Working…" : revoking ? "Revoke worker" : "Drain worker"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}