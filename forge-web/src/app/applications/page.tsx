"use client";

/**
 * Applications (UI.md §1, `redesign.md` §D).
 *
 * PowerJob's defining idea is that work is grouped under an *application*, and
 * this is the screen that creates and inspects those groupings. It is the first
 * stop in the navigation because it is the first question about a platform
 * serving more than one product: which application's jobs are failing?
 */

import Link from "next/link";
import { useState } from "react";
import { Plus, RefreshCw, Trash2 } from "lucide-react";

import { api } from "@/lib/api";
import { useQuery } from "@/lib/useQuery";
import { useAuth } from "@/lib/auth";
import { useToast } from "@/lib/useToast";
import type { Application } from "@/lib/types";
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
  DataTable,
  DataTableCell,
  DataTableHead,
  NumCell,
  PageHeader,
  RowLink,
  TableFooter,
  TableSkeleton,
} from "@/components/page";
import { EmptyState, ErrorState, ForbiddenState } from "@/components/states";

export default function ApplicationsPage() {
  const { session } = useAuth();
  const query = useQuery<{ applications: Application[] }>("/applications");
  const canWrite = session?.role === "OWNER" || session?.role === "ADMIN";

  const rows = query.data?.applications ?? [];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Applications"
        description="Groups related jobs and workers by product."
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
            {canWrite ? <CreateApplication onCreated={query.reload} /> : null}
          </>
        }
      />

      <div className="min-h-0 flex-1 overflow-auto">
        {query.state === "loading" ? (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead>Application</DataTableHead>
                <DataTableHead>Slug</DataTableHead>
                <DataTableHead>Description</DataTableHead>
                <DataTableHead className="w-24" align="right">
                  Jobs
                </DataTableHead>
                {canWrite ? (
                  <DataTableHead className="w-20" align="right">
                    Actions
                  </DataTableHead>
                ) : null}
              </TableRow>
            </TableHeader>
            <TableSkeleton rows={6} columns={canWrite ? 5 : 4} />
          </DataTable>
        ) : query.state === "error" ? (
          query.forbidden ? (
            <ForbiddenState />
          ) : (
            <ErrorState error={query.error} onRetry={query.reload} />
          )
        ) : rows.length === 0 ? (
          <EmptyState
            title="No applications yet"
            description="An application groups the jobs and workers for one product, so you can ask which application's jobs are failing rather than filtering a flat list."
            action={canWrite ? <CreateApplication onCreated={query.reload} /> : null}
          />
        ) : (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead>Application</DataTableHead>
                <DataTableHead className="w-48">Slug</DataTableHead>
                <DataTableHead>Description</DataTableHead>
                <DataTableHead className="w-24" align="right">
                  Jobs
                </DataTableHead>
                {canWrite ? (
                  <DataTableHead className="w-20" align="right">
                    Actions
                  </DataTableHead>
                ) : null}
              </TableRow>
            </TableHeader>

            <TableBody>
              {rows.map((application) => (
                <TableRow key={application.id} className="h-9">
                  <DataTableCell className="font-medium">
                    <RowLink href={`/applications/${application.id}`}>
                      {application.name}
                    </RowLink>
                  </DataTableCell>

                  <DataTableCell>
                    <code className="font-mono text-[11.5px] text-muted-foreground">
                      {application.slug}
                    </code>
                  </DataTableCell>

                  <DataTableCell className="text-[12.5px] text-muted-foreground">
                    {application.description ? (
                      <span
                        className="block max-w-[32rem] truncate"
                        title={application.description}
                      >
                        {application.description}
                      </span>
                    ) : (
                      <span className="text-muted-foreground/60">—</span>
                    )}
                  </DataTableCell>

                  <NumCell className="text-[12.5px]">{application.job_count}</NumCell>

                  {canWrite ? (
                    <DataTableCell align="right">
                      <DeleteApplication
                        application={application}
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

/**
 * Create an application.
 *
 * The slug is optional and derived from the name, so an operator types
 * "Payments API (EU)" and gets `payments-api-eu` rather than being asked to
 * invent an identifier. A slug they *do* supply is validated by the server
 * rather than normalised here, because the caller named something specific.
 */
function CreateApplication({ onCreated }: { onCreated: () => void }) {
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [slug, setSlug] = useState("");
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
      await api.post<Application>("/applications", {
        name: name.trim(),
        slug: slug.trim() || undefined,
        description: description.trim() || undefined,
      });
      toast.success(`Created ${name.trim()}`);
      setOpen(false);
      setName("");
      setSlug("");
      setDescription("");
      onCreated();
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Could not create the application.",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger
        render={
          <Button size="sm">
            <Plus aria-hidden />
            New application
          </Button>
        }
      />
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create an application</DialogTitle>
          <DialogDescription>
            Groups the jobs and workers for one product.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={submit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="app-name">Name</Label>
            <Input
              id="app-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              autoFocus
              placeholder="Payments API"
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="app-slug">Slug</Label>
            <Input
              id="app-slug"
              value={slug}
              onChange={(e) => setSlug(e.target.value)}
              placeholder="payments-api"
            />
            <p className="text-xs text-muted-foreground">
              Optional. Derived from the name when left empty. Lowercase letters,
              numbers and single hyphens.
            </p>
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="app-description">Description</Label>
            <Input
              id="app-description"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="What this application is responsible for"
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
              {busy ? "Creating…" : "Create application"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/**
 * Deletes an application.
 *
 * Refused by the server while jobs still reference it, and the refusal names
 * the count rather than saying "cannot delete" — an operator needs to know how
 * many jobs to move first.
 */
function DeleteApplication({
  application,
  onDeleted,
}: {
  application: Application;
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
      await api.delete(`/applications/${application.id}`);
      toast.success(`Deleted ${application.name}`);
      setOpen(false);
      onDeleted();
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "Could not delete the application.";
      setError(message);
      // The dialog stays open on refusal: closing it would hide the reason.
    } finally {
      setBusy(false);
    }
  }

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label={`Delete ${application.name}`}
        onClick={() => setOpen(true)}
      >
        <Trash2 aria-hidden />
      </Button>

      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete “{application.name}”?</AlertDialogTitle>
          <AlertDialogDescription>
            {application.job_count > 0 ? (
              <>
                {application.job_count} job
                {application.job_count === 1 ? "" : "s"} still belong
                {application.job_count === 1 ? "s" : ""} to this application. Move
                them first — deletion is refused while any are attached.
              </>
            ) : (
              <>
                The application is deleted. Its jobs are not: they become
                ungrouped rather than being destroyed with the container.
              </>
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
            variant="destructive"
            disabled={busy}
            onClick={remove}
          >
            {busy ? "Deleting…" : "Delete application"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
