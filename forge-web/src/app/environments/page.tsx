"use client";

/**
 * Environments (`redesign.md` §2.G, §3).
 *
 * An environment is *where* work runs. It is the axis a job migrates along and
 * the thing a change guardrail reads, so this screen leads with the distinction
 * that matters: which environments are protected, and what `production` means
 * for anything changed inside one.
 */

import { useState } from "react";
import { AlertTriangle, Lock, Plus, RefreshCw, Trash2 } from "lucide-react";

import { api } from "@/lib/api";
import { useQuery } from "@/lib/useQuery";
import { useAuth } from "@/lib/auth";
import { useToast } from "@/lib/useToast";
import type { Environment, EnvironmentKind } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { TableBody, TableHeader, TableRow } from "@/components/ui/table";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
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
  Toolbar,
  TableFooter,
  TableSkeleton,
} from "@/components/page";
import { EmptyState, ErrorState, ForbiddenState } from "@/components/states";

const KINDS: { value: EnvironmentKind; label: string; note: string }[] = [
  { value: "development", label: "Development", note: "Local and CI-run work." },
  { value: "staging", label: "Staging", note: "Verifies the production shape." },
  {
    value: "production",
    label: "Production",
    note: "Customer-facing. Changes need explicit confirmation.",
  },
  { value: "other", label: "Other", note: "A customer deployment or sandbox." },
];

export default function EnvironmentsPage() {
  const { session } = useAuth();
  const query = useQuery<{ environments: Environment[] }>("/environments");
  const canWrite = session?.role === "OWNER" || session?.role === "ADMIN";

  const rows = query.data?.environments ?? [];
  const production = rows.find((row) => row.protected);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Environments"
        description="Where work runs. A job's identity travels; its bindings do not."
        actions={
          <>
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
            {canWrite ? <CreateEnvironment onCreated={query.reload} /> : null}
          </>
        }
      />

      {/*
        The production guardrail, stated once at the top rather than repeated in
        a tooltip on every cell. `protected` comes from the server, so the rule
        does not depend on this client implementing it correctly.
      */}
      {production ? (
        <div className="flex items-start gap-2 border-b border-warning/40 bg-warning/10 px-4 py-2">
          <Lock className="mt-0.5 size-3.5 shrink-0 text-warning-foreground" aria-hidden />
          <p className="text-[12.5px] text-warning-foreground">
            <strong className="font-semibold">
              {production.name} is protected.
            </strong>{" "}
            Destructive changes inside it ask for confirmation, and a tenant may
            have only one production environment so a change guardrail cannot
            protect one and silently skip another.
          </p>
        </div>
      ) : null}

      <Toolbar>
        <p className="text-[12px] text-muted-foreground">
          {rows.length} environment{rows.length === 1 ? "" : "s"}
          {production ? ` · protected: ${production.slug}` : ""}
        </p>
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-auto">
        {query.state === "loading" ? (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead>Environment</DataTableHead>
                <DataTableHead className="w-40">Slug</DataTableHead>
                <DataTableHead className="w-36">Kind</DataTableHead>
                <DataTableHead>Description</DataTableHead>
                {canWrite ? (
                  <DataTableHead className="w-20" align="right">
                    Actions
                  </DataTableHead>
                ) : null}
              </TableRow>
            </TableHeader>
            <TableSkeleton rows={4} columns={canWrite ? 5 : 4} />
          </DataTable>
        ) : query.state === "error" ? (
          query.forbidden ? (
            <ForbiddenState />
          ) : (
            <ErrorState error={query.error} onRetry={query.reload} />
          )
        ) : rows.length === 0 ? (
          <EmptyState
            title="No environments yet"
            description="An environment decides which workers, queues and secrets a job may reach, and it is the axis a job migrates along."
            action={canWrite ? <CreateEnvironment onCreated={query.reload} /> : null}
          />
        ) : (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead>Environment</DataTableHead>
                <DataTableHead className="w-40">Slug</DataTableHead>
                <DataTableHead className="w-36">Kind</DataTableHead>
                <DataTableHead>Description</DataTableHead>
                {canWrite ? (
                  <DataTableHead className="w-20" align="right">
                    Actions
                  </DataTableHead>
                ) : null}
              </TableRow>
            </TableHeader>

            <TableBody>
              {rows.map((environment) => (
                <TableRow key={environment.id} className="h-9">
                  <DataTableCell className="font-medium">
                    <span className="flex items-center gap-2">
                      {environment.name}
                      {environment.protected ? (
                        <span className="inline-flex items-center gap-1 text-[10.5px] font-medium tracking-wide text-warning-foreground uppercase">
                          <Lock className="size-2.5" aria-hidden />
                          Protected
                        </span>
                      ) : null}
                    </span>
                  </DataTableCell>

                  <DataTableCell>
                    <code className="font-mono text-[11.5px] text-muted-foreground">
                      {environment.slug}
                    </code>
                  </DataTableCell>

                  <DataTableCell className="text-[12.5px] capitalize">
                    {environment.kind}
                  </DataTableCell>

                  <DataTableCell className="text-[12.5px] text-muted-foreground">
                    {environment.description ? (
                      <span
                        className="block max-w-[32rem] truncate"
                        title={environment.description}
                      >
                        {environment.description}
                      </span>
                    ) : (
                      <span className="text-muted-foreground/60">—</span>
                    )}
                  </DataTableCell>

                  {canWrite ? (
                    <DataTableCell align="right">
                      <DeleteEnvironment
                        environment={environment}
                        onDeleted={query.reload}
                      />
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

function CreateEnvironment({ onCreated }: { onCreated: () => void }) {
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
  const [kind, setKind] = useState<EnvironmentKind>("development");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    if (!name.trim()) {
      setError("A name is required.");
      return;
    }
    setBusy(true);
    try {
      await api.post<Environment>("/environments", {
        name: name.trim(),
        kind,
        slug: slug.trim() || undefined,
        description: description.trim() || undefined,
      });
      toast.success(`Created ${name.trim()}`);
      setOpen(false);
      setName("");
      setSlug("");
      setDescription("");
      setKind("development");
      onCreated();
    } catch (cause) {
      setError(
        cause instanceof Error
          ? cause.message
          : "Could not create the environment.",
      );
    } finally {
      setBusy(false);
    }
  }

  const selected = KINDS.find((entry) => entry.value === kind);

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger
        render={
          <Button size="sm">
            <Plus aria-hidden />
            New environment
          </Button>
        }
      />
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create an environment</DialogTitle>
          <DialogDescription>
            Decides which workers, queues and secrets a job in it may reach.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={submit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="env-name">Name</Label>
            <Input
              id="env-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              autoFocus
              placeholder="Production"
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="env-kind">Kind</Label>
            <Select value={kind} onValueChange={(value) => setKind(value as EnvironmentKind)}>
              <SelectTrigger id="env-kind">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {KINDS.map((entry) => (
                  <SelectItem key={entry.value} value={entry.value}>
                    {entry.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {kind === "production" ? (
              <p className="flex items-start gap-1.5 text-xs text-warning-foreground">
                <AlertTriangle className="mt-0.5 size-3 shrink-0" aria-hidden />
                A tenant may have only one production environment, so a change
                guardrail cannot protect one and silently skip another.
              </p>
            ) : (
              <p className="text-xs text-muted-foreground">{selected?.note}</p>
            )}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="env-slug">Slug</Label>
            <Input
              id="env-slug"
              value={slug}
              onChange={(e) => setSlug(e.target.value)}
              placeholder="prod"
            />
            <p className="text-xs text-muted-foreground">
              Optional. Derived from the name when left empty.
            </p>
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="env-description">Description</Label>
            <Input
              id="env-description"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </div>

          {error ? (
            <p role="alert" className="text-xs text-danger-foreground">
              {error}
            </p>
          ) : null}

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy}>
              {busy ? "Creating…" : "Create environment"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function DeleteEnvironment({
  environment,
  onDeleted,
}: {
  environment: Environment;
  onDeleted: () => void;
}) {
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function remove() {
    setBusy(true);
    setError(null);
    try {
      await api.delete(`/environments/${environment.id}`);
      toast.success(`Deleted ${environment.name}`);
      setOpen(false);
      onDeleted();
    } catch (cause) {
      const message =
        cause instanceof Error
          ? cause.message
          : "Could not delete the environment.";
      setError(message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={`Delete ${environment.name}`}
        onClick={() => setOpen(true)}
      >
        <Trash2 aria-hidden />
      </Button>

      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete “{environment.name}”?</AlertDialogTitle>
          <AlertDialogDescription>
            {environment.protected ? (
              <>
                This is the tenant&apos;s production environment. Deleting it
                would leave its jobs with nowhere to run.
              </>
            ) : (
              "Jobs still assigned to it keep it, but deletion is refused until you move them elsewhere."
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>

        {error ? (
          <p role="alert" className="text-xs text-danger-foreground">
            {error}
          </p>
        ) : null}

        <AlertDialogFooter>
          <AlertDialogAction variant="outline" onClick={() => setOpen(false)}>
            Cancel
          </AlertDialogAction>
          <AlertDialogAction
            variant={environment.protected ? "destructive" : "default"}
            disabled={busy}
            onClick={remove}
          >
            {busy ? "Deleting…" : "Delete environment"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
