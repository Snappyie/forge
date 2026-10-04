"use client";

/**
 * Job list (spec 7.5, UI.md section 5).
 *
 * The screen an operator opens most, so it is built for scanning rather than
 * for reading: a full-bleed table, one row per job, and every column a value
 * they actually compare. Filtering happens client-side over the loaded page, so
 * typing is instant and never waits on a round trip.
 */

import { useMemo, useState } from "react";
import { Archive, Loader2, Play, Plus, RefreshCw, Search, X } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { usePaginatedQuery } from "@/lib/useQuery";
import { compare, useTablePrefs } from "@/lib/tablePrefs";
import { useToast } from "@/lib/useToast";
import {
  formatRelative,
  formatTimestamp,
  roleCan,
  type Job,
  type Role,
} from "@/lib/types";
import { BulkActions } from "@/components/ui/bulk-actions";
import { ImportExport } from "@/components/ui/import-export";
import { Checkbox } from "@/components/ui/checkbox";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  TableBody,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
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
  NumCell,
  PinnedCell,
  PageHeader,
  RowLink,
  Toolbar,
  TableFooter,
  TableSkeleton,
} from "@/components/page";
import { PriorityBadge, StatusCell } from "@/components/status-badge";
import { EmptyState, ErrorState, ForbiddenState } from "@/components/states";

export default function JobsPage() {
  const { session } = useAuth();
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("ALL");
  const [selected, setSelected] = useState<string[]>([]);
  const { prefs, toggleSort } = useTablePrefs("jobs");

  const [pageSize, setPageSize] = useState(50);
  const query = usePaginatedQuery<Job>("/jobs", pageSize);
  // Controlled so the empty state can offer "New job" itself, rather than only
  // describing what the user could do.
  const [createOpen, setCreateOpen] = useState(false);

  const filtered = useMemo(() => {
    const rows = query.data ?? [];
    return rows.filter((job) => {
      if (status !== "ALL" && job.status !== status) return false;
      if (!search.trim()) return true;
      const needle = search.trim().toLowerCase();
      return (
        job.name.toLowerCase().includes(needle) ||
        (job.key ?? "").toLowerCase().includes(needle)
      );
    });
  }, [query.data, search, status]);

  const sorted = useMemo(() => {
    const copy = [...filtered];
    copy.sort((a, b) => {
      const record = a as unknown as Record<string, unknown>;
      const other = b as unknown as Record<string, unknown>;
      const result = compare(record[prefs.sortKey], other[prefs.sortKey]);
      return prefs.sortDirection === "asc" ? result : -result;
    });
    return copy;
  }, [filtered, prefs.sortKey, prefs.sortDirection]);

  const filtering = search.trim().length > 0 || status !== "ALL";
  const canWrite = roleCan(session?.role, "jobs:write");
  const allSelected =
    sorted.length > 0 && selected.length === sorted.length;

  function clearFilters() {
    setSearch("");
    setStatus("ALL");
  }

  const columns = [
    ["name", "Name"],
    ["status", "Status"],
    ["priority", "Priority"],
    ["updated_at", "Updated"],
  ] as const;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Jobs"
        description="Definitions, versions, and schedules."
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
            {canWrite ? (
              <CreateJobDialog
                onCreated={query.reload}
                open={createOpen}
                onOpenChange={setCreateOpen}
              />
            ) : null}
          </>
        }
      />

      <Toolbar>
        <div className="relative min-w-[14rem] flex-1 sm:max-w-xs">
          <Search
            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
            aria-hidden
          />
          <Input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Filter by name or key"
            aria-label="Filter jobs"
            className="h-7 pl-7 text-[12.5px]"
          />
        </div>

        <Select value={status} onValueChange={(value) => setStatus(value ?? "ALL")}>
          <SelectTrigger
            size="sm"
            className="h-7 w-32 text-[12.5px]"
            aria-label="Filter by status"
          >
            <SelectValue placeholder="Status" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="ALL">All statuses</SelectItem>
            <SelectItem value="DRAFT">Draft</SelectItem>
            <SelectItem value="ACTIVE">Active</SelectItem>
            <SelectItem value="ARCHIVED">Archived</SelectItem>
          </SelectContent>
        </Select>

        {filtering ? (
          <Button
            variant="ghost"
            size="sm"
            onClick={clearFilters}
            className="h-7 text-[12.5px] text-muted-foreground"
          >
            <X aria-hidden />
            Clear filters
          </Button>
        ) : null}

        <div className="ml-auto flex items-center gap-2">
          <ImportExport onImported={query.reload} />
          <label htmlFor="page-size" className="sr-only">
            Rows per page
          </label>
          <Select
            value={String(pageSize)}
            onValueChange={(value) => value && setPageSize(Number(value))}
          >
            <SelectTrigger
              size="sm"
              id="page-size"
              className="h-7 w-24 text-[12.5px]"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {[25, 50, 100].map((size) => (
                <SelectItem key={size} value={String(size)}>
                  {size} / page
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </Toolbar>

      {selected.length > 0 ? (
        <div className="border-b border-border bg-accent/40 px-4 py-2">
          <BulkActions
            selected={selected}
            onDone={() => {
              setSelected([]);
              query.reload();
            }}
          />
        </div>
      ) : null}

      <div className="min-h-0 flex-1 overflow-auto">
        {query.state === "loading" ? (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead className="w-8" />
                {columns.map(([, label]) => (
                  <DataTableHead key={label}>{label}</DataTableHead>
                ))}
                <DataTableHead className="w-8" />
                <DataTableHead className="w-20" align="right">
                  Actions
                </DataTableHead>
              </TableRow>
            </TableHeader>
            <TableSkeleton rows={12} columns={7} />
          </DataTable>
        ) : query.state === "error" ? (
          query.forbidden ? (
            <ForbiddenState />
          ) : (
            <ErrorState error={query.error} onRetry={query.reload} />
          )
        ) : sorted.length === 0 ? (
          <EmptyState
            title={filtering ? "No jobs match these filters" : "No jobs yet"}
            description={
              filtering
                ? "Nothing here matches. Clear the filters to see every job in this tenant."
                : "A job is a reusable definition of work this tenant runs. Create one to get started."
            }
            action={
              filtering ? (
                <Button size="sm" variant="outline" onClick={clearFilters}>
                  Clear filters
                </Button>
              ) : canWrite ? (
                <Button size="sm" onClick={() => setCreateOpen(true)}>
                  <Plus aria-hidden />
                  Create your first job
                </Button>
              ) : null
            }
          />
        ) : (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead className="w-8">
                  <Checkbox
                    aria-label="Select all listed jobs"
                    checked={allSelected}
                    onCheckedChange={(checked) =>
                      setSelected(checked ? sorted.map((j) => j.id) : [])
                    }
                  />
                </DataTableHead>
                {columns.map(([key, label]) => (
                  <DataTableHead key={key}>
                    <button
                      type="button"
                      onClick={() => toggleSort(key)}
                      aria-label={`Sort by ${label}`}
                      className="inline-flex items-center gap-1 hover:text-foreground"
                    >
                      {label}
                      {prefs.sortKey === key ? (
                        <span aria-hidden>
                          {prefs.sortDirection === "asc" ? "▲" : "▼"}
                        </span>
                      ) : null}
                    </button>
                  </DataTableHead>
                ))}
                <DataTableHead className="w-8" />
                <DataTableHead className="w-20" align="right">
                  Actions
                </DataTableHead>
              </TableRow>
            </TableHeader>

            <TableBody>
              {sorted.map((job) => (
                <TableRow key={job.id} className="h-8">
                  <DataTableCell className="w-8">
                    <Checkbox
                      aria-label={`Select ${job.name}`}
                      checked={selected.includes(job.id)}
                      onCheckedChange={(checked) =>
                        setSelected((current) =>
                          checked
                            ? [...current, job.id]
                            : current.filter((id) => id !== job.id),
                        )
                      }
                    />
                  </DataTableCell>

                  <PinnedCell>
                    <span className="flex items-baseline gap-2">
                      <RowLink href={`/jobs/${job.id}`}>{job.name}</RowLink>
                      {job.key ? (
                        <code className="font-mono text-[11px] text-muted-foreground">
                          {job.key}
                        </code>
                      ) : null}
                    </span>
                  </PinnedCell>

                  <DataTableCell>
                    <StatusCell status={job.status} />
                  </DataTableCell>

                  <DataTableCell>
                    <PriorityBadge priority={job.priority} />
                  </DataTableCell>

                  <NumCell
                    className="text-[12px] text-muted-foreground"
                    title={formatTimestamp(job.updated_at)}
                  >
                    {formatRelative(job.updated_at)}
                  </NumCell>

                  <DataTableCell className="w-8" />

                  <DataTableCell align="right" className="w-20">
                    <RowActions
                      job={job}
                      role={session?.role}
                      onDone={query.reload}
                    />
                  </DataTableCell>
                </TableRow>
              ))}
            </TableBody>
          </DataTable>
        )}
      </div>

      <TableFooter
        shown={sorted.length}
        total={query.data?.length}
        hasMore={query.page?.has_more}
      >
        {query.page?.has_more ? (
          <Button
            variant="outline"
            size="sm"
            onClick={query.loadMore}
            disabled={query.state === "loading"}
          >
            Load more
          </Button>
        ) : null}
      </TableFooter>
    </div>
  );
}

function RowActions({
  job,
  role,
  onDone,
}: {
  job: Job;
  role: Role | undefined;
  onDone: () => void;
}) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  async function run() {
    setBusy(true);
    try {
      // A generated idempotency key makes a double-click harmless.
      const idempotencyKey = crypto.randomUUID();
      const created = await api.post<{ execution_id?: string; id?: string }>(
        `/jobs/${job.id}/trigger`,
        {},
        idempotencyKey,
      );
      const executionId = created.execution_id ?? created.id;
      toast.success("Execution queued", {
        label: "Open execution",
        onClick: () => window.location.assign(`/executions/${executionId}`),
      });
      onDone();
    } catch (cause) {
      toast.error(
        `Could not run ${job.name}`,
        cause instanceof ApiError ? cause.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  async function archive() {
    setBusy(true);
    try {
      await api.delete(`/jobs/${job.id}`);
      toast.success(`${job.name} archived`);
      onDone();
    } catch (cause) {
      toast.error(
        `Could not archive ${job.name}`,
        cause instanceof ApiError ? cause.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex items-center justify-end gap-0.5">
      {roleCan(role, "jobs:trigger") ? (
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={busy}
          aria-label={`Run ${job.name} now`}
          title="Run now"
          onClick={run}
        >
          {busy ? (
            <Loader2 className="animate-spin" aria-hidden />
          ) : (
            <Play aria-hidden />
          )}
        </Button>
      ) : null}

      {roleCan(role, "jobs:delete") && job.status !== "ARCHIVED" ? (
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={busy}
          aria-label={`Archive ${job.name}`}
          title="Archive"
          onClick={archive}
        >
          <Archive aria-hidden />
        </Button>
      ) : null}
    </div>
  );
}

function CreateJobDialog({
  onCreated,
  open: controlledOpen,
  onOpenChange,
}: {
  onCreated: () => void;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  // Controlled when a parent supplies the state (so the empty state can open
  // this), otherwise the trigger manages it internally.
  const [uncontrolledOpen, setUncontrolledOpen] = useState(false);
  const isControlled = controlledOpen !== undefined;
  const open = isControlled ? controlledOpen : uncontrolledOpen;
  const setOpen = (next: boolean) => {
    if (!isControlled) setUncontrolledOpen(next);
    onOpenChange?.(next);
  };
  const [name, setName] = useState("");
  const [key, setKey] = useState("");
  const [priority, setPriority] = useState("NORMAL");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Checked while typing so the reason a field is rejected appears where the
  // user is looking, rather than only as a native tooltip on submit.
  const trimmedName = name.trim();
  const trimmedKey = key.trim();
  const nameProblem =
    trimmedName.length === 0
      ? "Give the job a name."
      : trimmedName.length > 200
        ? "Keep the name under 200 characters."
        : null;
  const keyProblem =
    trimmedKey.length > 0 && !/^[a-zA-Z0-9._-]+$/.test(trimmedKey)
      ? "Use letters, digits, dots, dashes, or underscores."
      : null;
  const formProblem = nameProblem ?? keyProblem;

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    // Guard here as well as on the button, so the form cannot be submitted by
    // pressing Enter with an invalid field.
    if (formProblem) return;
    setBusy(true);
    try {
      // A client-generated idempotency key means a retried submit creates one
      // job rather than two (spec 02.15).
      const idempotencyKey = crypto.randomUUID();
      await api.post(
        "/jobs",
        { name: trimmedName, key: trimmedKey || undefined, priority },
        idempotencyKey,
      );
      setOpen(false);
      setName("");
      setKey("");
      onCreated();
    } catch (cause) {
      setError(
        cause instanceof ApiError
          ? (cause.fieldError("name") ?? cause.message)
          : "Could not reach the server.",
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
            New job
          </Button>
        }
      />
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create a job</DialogTitle>
          <DialogDescription>
            A job is a reusable definition. Publish a version before it can run.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={submit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="job-name">Name</Label>
            <Input
              id="job-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              autoFocus
              aria-invalid={nameProblem ? true : undefined}
              aria-describedby={nameProblem ? "job-name-problem" : undefined}
            />
            {nameProblem && name.length > 0 ? (
              <p id="job-name-problem" className="text-xs text-danger-foreground">
                {nameProblem}
              </p>
            ) : null}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="job-key">Key</Label>
            <Input
              id="job-key"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder="stable-identifier"
              aria-invalid={keyProblem ? true : undefined}
              aria-describedby={keyProblem ? "job-key-problem" : undefined}
            />
            {keyProblem ? (
              <p id="job-key-problem" className="text-xs text-danger-foreground">
                {keyProblem}
              </p>
            ) : (
              <p className="text-xs text-muted-foreground">
                Optional. Unique within this tenant.
              </p>
            )}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="job-priority">Priority</Label>
            <Select
              value={priority}
              onValueChange={(value) => value && setPriority(value)}
            >
              <SelectTrigger id="job-priority" size="sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="CRITICAL">Critical</SelectItem>
                <SelectItem value="HIGH">High</SelectItem>
                <SelectItem value="NORMAL">Normal</SelectItem>
                <SelectItem value="LOW">Low</SelectItem>
                <SelectItem value="BACKGROUND">Background</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {error ? (
            <p role="alert" className="text-xs text-danger-foreground">
              {error}
            </p>
          ) : null}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setOpen(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={busy || formProblem !== null}>
              {busy ? "Creating…" : "Create job"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}