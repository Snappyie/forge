"use client";

/**
 * Job list (spec 7.5).
 *
 * Reads live data through the typed client, and provides the loading, empty,
 * error and permission-denied states spec 7.2 requires.
 */

import Link from "next/link";
import { useMemo, useState } from "react";
import { Archive, Loader2, Play, Plus, RefreshCw, Search } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { usePaginatedQuery } from "@/lib/useQuery";
import { cn } from "cn";
import { BulkActions } from "@/components/ui/bulk-actions";
import { ImportExport } from "@/components/ui/import-export";
import { Checkbox } from "@/components/ui/checkbox";
import { Spinner } from "@/components/ui/spinner";
import {
} from "@/components/ui/pagination";
import { compare, useTablePrefs } from "@/lib/tablePrefs";
import {
  formatRelative,
  formatTimestamp,
  roleCan,
  type Job,
  type Role,
} from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { PriorityBadge, StatusBadge } from "@/components/status-badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
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

export default function JobsPage() {
  const { session } = useAuth();
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("ALL");
  const [selected, setSelected] = useState<string[]>([]);
  const { prefs, toggleSort, setDensity } = useTablePrefs("jobs");

  const [pageSize, setPageSize] = useState(25);
  const query = usePaginatedQuery<Job>("/jobs", pageSize);
  // Controlled so the empty state can offer "New job" itself, rather than only
  // describing what the user could do.
  const [createOpen, setCreateOpen] = useState(false);
  const jobs = useMemo(() => {
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

  // A filter that matches nothing is an empty result, not an empty collection.
  const empty = query.state === "ready" && jobs.length === 0;
  const sorted = useMemo(() => {
    const copy = [...jobs];
    copy.sort((a, b) => {
      const record = a as unknown as Record<string, unknown>;
      const other = b as unknown as Record<string, unknown>;
      const result = compare(record[prefs.sortKey], other[prefs.sortKey]);
      return prefs.sortDirection === "asc" ? result : -result;
    });
    return copy;
  }, [jobs, prefs.sortKey, prefs.sortDirection]);

  const filtering = search.trim().length > 0 || status !== "ALL";

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Jobs</h1>
          <p className="text-xs text-muted-foreground">
            Definitions, versions, and schedules.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={query.reload} aria-label="Refresh">
            <RefreshCw className="size-3.5" aria-hidden />
            Refresh
          </Button>
          {roleCan(session?.role, "jobs:write") ? (
            <CreateJobDialog
              onCreated={query.reload}
              open={createOpen}
              onOpenChange={setCreateOpen}
            />
          ) : null}
        </div>
      </header>

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-[16rem] flex-1">
          <Search
            className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground"
            aria-hidden
          />
          <Input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Filter by name or key"
            aria-label="Filter jobs"
            className="pl-8"
          />
        </div>

        <Select value={status} onValueChange={(value) => setStatus(value ?? "ALL")}>
          <SelectTrigger className="w-36" aria-label="Filter by status">
            <SelectValue placeholder="Status" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="ALL">All statuses</SelectItem>
            <SelectItem value="DRAFT">Draft</SelectItem>
            <SelectItem value="ACTIVE">Active</SelectItem>
            <SelectItem value="ARCHIVED">Archived</SelectItem>
          </SelectContent>
        </Select>

        {/* UI.md section 5: density is a persistent preference. */}
        <label className="sr-only" htmlFor="page-size">
          Rows per page
        </label>
        <select
          id="page-size"
          value={String(pageSize)}
          onChange={(e) => setPageSize(Number(e.target.value))}
          className="h-8 rounded-md border border-border bg-background px-2 text-xs"
        >
          {[10, 25, 50, 100].map((size) => (
            <option key={size} value={size}>
              {size} / page
            </option>
          ))}
        </select>

        <div className="flex rounded-md border border-border">
          {(["compact", "comfortable"] as const).map((option) => (
            <button
              key={option}
              type="button"
              onClick={() => setDensity(option)}
              aria-pressed={prefs.density === option}
              aria-label={`${option} density`}
              className={cn(
                "px-2 py-1 text-[11px] capitalize",
                prefs.density === option
                  ? "bg-accent text-accent-foreground"
                  : "text-muted-foreground hover:bg-accent/50",
              )}
            >
              {option === "compact" ? "Dense" : "Roomy"}
            </button>
          ))}
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3 text-xs text-muted-foreground">
        <span>
          {sorted.length} loaded{query.page?.has_more ? " (more available)" : ""}
        </span>
        {query.page?.has_more ? (
          <button
            type="button"
            onClick={query.loadMore}
            disabled={query.state === "loading"}
            className="inline-flex items-center gap-1.5 rounded-md border border-border px-2 py-1 text-xs hover:bg-accent/50 disabled:opacity-50"
          >
            {query.state === "loading" ? <Spinner className="size-3" /> : null}
            Load more
          </button>
        ) : null}
      </div>

      <ImportExport onImported={query.reload} />

      <BulkActions
        selected={selected}
        onDone={() => {
          setSelected([]);
          query.reload();
        }}
      />

      <div className="rounded-lg border border-border">
        <AsyncBoundary
          state={query.state}
          error={query.error}
          forbidden={query.forbidden}
          empty={empty}
          onRetry={query.reload}
          loadingLabel="Loading jobs"
          emptyTitle={filtering ? "No jobs match this filter" : "No jobs yet"}
          emptyDescription={
            filtering
              ? "Try a different search or clear the status filter."
              : "Create a job to describe the work this tenant runs."
          }
          emptyAction={
            filtering ? null : (
              <Button size="sm" variant="outline" onClick={() => setCreateOpen(true)}>
                <Plus className="size-3.5" aria-hidden />
                New job
              </Button>
            )
          }
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-8">
                  <Checkbox
                    aria-label="Select all listed jobs"
                    checked={
                      sorted.length > 0 && selected.length === sorted.length
                    }
                    onCheckedChange={(checked) =>
                      setSelected(checked ? sorted.map((j) => j.id) : [])
                    }
                  />
                </TableHead>
                {(
                  [
                    ["name", "Name"],
                    ["status", "Status"],
                    ["priority", "Priority"],
                    ["current_version_id", "Version"],
                    ["updated_at", "Updated"],
                  ] as const
                ).map(([key, label]) => (
                  <TableHead key={key}>
                    <button
                      type="button"
                      onClick={() => toggleSort(key)}
                      aria-label={`Sort by ${label}`}
                      className="flex items-center gap-1 hover:text-foreground"
                    >
                      {label}
                      {prefs.sortKey === key ? (
                        <span aria-hidden>
                          {prefs.sortDirection === "asc" ? "▲" : "▼"}
                        </span>
                      ) : null}
                    </button>
                  </TableHead>
                ))}
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sorted.map((job) => (
                <TableRow
                  key={job.id}
                  className={prefs.density === "compact" ? "h-8" : undefined}
                >
                  <TableCell>
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
                  </TableCell>
                  <TableCell>
                    <Link
                      href={`/jobs/${job.id}`}
                      className="font-medium underline-offset-4 hover:underline"
                    >
                      {job.name}
                    </Link>
                    {job.key ? (
                      <code className="ml-2 text-xs text-muted-foreground">{job.key}</code>
                    ) : null}
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={job.status} />
                  </TableCell>
                  <TableCell>
                    <PriorityBadge priority={job.priority} />
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {job.current_version_id ? (
                      <code>{job.current_version_id.slice(0, 8)}</code>
                    ) : (
                      "—"
                    )}
                  </TableCell>
                  <TableCell
                    className="text-xs text-muted-foreground"
                    title={formatTimestamp(job.updated_at)}
                  >
                    {formatRelative(job.updated_at)}
                  </TableCell>
                  <TableCell className="text-right">
                    <RowActions job={job} role={session?.role} onDone={query.reload} />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </AsyncBoundary>
      </div>
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
  const [busy, setBusy] = useState(false);

  async function run() {
    setBusy(true);
    try {
      // A generated idempotency key makes a double-click harmless.
      const idempotencyKey = crypto.randomUUID ? crypto.randomUUID() : Math.random().toString(36).substring(2) + Date.now().toString(36);
      await api.post(`/jobs/${job.id}/trigger`, {}, idempotencyKey);
      onDone();
    } catch (cause) {
      window.alert(cause instanceof ApiError ? cause.message : "the request failed");
    } finally {
      setBusy(false);
    }
  }

  async function archive() {
    if (!window.confirm(`Archive "${job.name}"? It will stop being scheduled.`)) return;
    setBusy(true);
    try {
      await api.delete(`/jobs/${job.id}`);
      onDone();
    } catch (cause) {
      window.alert(cause instanceof ApiError ? cause.message : "the request failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex items-center justify-end gap-1">
      {roleCan(role, "jobs:trigger") ? (
        <Button
          variant="ghost"
          size="icon"
          disabled={busy}
          aria-label={`Run ${job.name}`}
          title="Run now"
          onClick={run}
        >
          {busy ? (
            <Loader2 className="size-3.5 animate-spin" aria-hidden />
          ) : (
            <Play className="size-3.5" aria-hidden />
          )}
        </Button>
      ) : null}

      {roleCan(role, "jobs:delete") && job.status !== "ARCHIVED" ? (
        <Button
          variant="ghost"
          size="icon"
          disabled={busy}
          aria-label={`Archive ${job.name}`}
          title="Archive"
          onClick={archive}
        >
          <Archive className="size-3.5" aria-hidden />
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
      const idempotencyKey = crypto.randomUUID ? crypto.randomUUID() : Math.random().toString(36).substring(2) + Date.now().toString(36);
      await api.post(
        "/jobs",
        { name: name.trim(), key: key.trim() || undefined, priority },
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
          : "could not reach the server",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button size="sm">
          <Plus className="size-3.5" aria-hidden />
          New job
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create a job</DialogTitle>
          <DialogDescription>
            A job is a reusable definition; publish a version before running it.
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
              <p id="job-name-problem" className="text-xs text-destructive">
                {nameProblem}
              </p>
            ) : null}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="job-key">Key (optional)</Label>
            <Input
              id="job-key"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder="stable-identifier"
              aria-invalid={keyProblem ? true : undefined}
              aria-describedby={keyProblem ? "job-key-problem" : undefined}
            />
            {keyProblem ? (
              <p id="job-key-problem" className="text-xs text-destructive">
                {keyProblem}
              </p>
            ) : (
              <p className="text-xs text-muted-foreground">
                Unique within this tenant.
              </p>
            )}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="job-priority">Priority</Label>
            <Select value={priority} onValueChange={(value) => value && setPriority(value)}>
              <SelectTrigger id="job-priority">
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
            <p role="alert" className="text-xs text-destructive">
              {error}
            </p>
          ) : null}

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={busy || formProblem !== null}>
              {busy ? "Creating…" : "Create"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
