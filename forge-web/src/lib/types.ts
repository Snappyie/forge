/**
 * Types mirroring the API's DTOs.
 *
 * These replace the `useState<any[]>` state the pages carried, so a field the
 * server stops sending becomes a compile error rather than an `undefined` at
 * runtime.
 */

/** Spec 05 §5.1 list envelope. */
export interface ListEnvelope<T> {
  data: T[];
  page: { next_cursor: string | null; has_more: boolean };
  request_id: string;
}

/** Spec 02.2. */
export interface Job {
  id: string;
  key: string | null;
  name: string;
  description: string | null;
  status: JobStatus;
  current_version_id: string | null;
  default_queue_id: string | null;
  priority: Priority;
  owner_id: string | null;
  labels: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export type JobStatus = "DRAFT" | "ACTIVE" | "ARCHIVED";

/** Spec 02.13. */
export type Priority = "CRITICAL" | "HIGH" | "NORMAL" | "LOW" | "BACKGROUND";

/** Spec 02.3. */
export interface JobVersion {
  id: string;
  job_id?: string;
  version_number: number;
  execution_type: ExecutionType;
  timeout_seconds?: number;
  published_at: string | null;
  created_at: string;
}

export type ExecutionType = "HTTP_REQUEST" | "CONTAINER_COMMAND" | "WORKER_TASK";

/** Spec 02.5. */
export type ExecutionStatus =
  | "SCHEDULED"
  | "QUEUED"
  | "DISPATCHED"
  | "RUNNING"
  | "SUCCEEDED"
  | "FAILED"
  | "TIMED_OUT"
  | "CANCEL_REQUESTED"
  | "CANCELLED"
  | "RETRY_SCHEDULED"
  | "DEAD_LETTERED"
  | "ABANDONED";

/** Spec 01.6. */
export interface Execution {
  id: string;
  job_id: string;
  job_version_id: string;
  workflow_id: string | null;
  status: ExecutionStatus;
  queue_id: string | null;
  worker_id: string | null;
  attempt_count: number;
  priority: Priority;
  trigger_source: TriggerSource;
  scheduled_for: string | null;
  correlation_id: string | null;
  error_class: string | null;
  error_message: string | null;
  created_at: string;
  started_at: string | null;
  ended_at: string | null;
}

export type TriggerSource =
  | "SCHEDULE"
  | "MANUAL"
  | "API"
  | "WORKFLOW"
  | "RETRY"
  | "RECOVERY";

/** Spec 02.8. */
export type WorkerStatus =
  | "REGISTERING"
  | "READY"
  | "BUSY"
  | "DRAINING"
  | "OFFLINE"
  | "REVOKED";

/** Spec 10.7. */
export interface Worker {
  id: string;
  name: string | null;
  hostname: string;
  version: string | null;
  status: WorkerStatus;
  capabilities: unknown;
  labels: Record<string, unknown>;
  draining: boolean;
  last_heartbeat_at: string;
  registered_at: string | null;
}

/** Spec 02.4. */
export interface Schedule {
  id: string;
  target_id: string;
  target_type: "JOB" | "WORKFLOW";
  expression: string | null;
  timezone: string;
  misfire_policy: MisfirePolicy;
  catch_up_limit: number;
  enabled: boolean;
  next_run_at: string | null;
  last_run_at: string | null;
  created_at?: string;
}

export type MisfirePolicy = "SKIP" | "FIRE_ONCE" | "CATCH_UP";

/** Spec 02.10. */
export interface Workflow {
  id: string;
  key: string | null;
  name: string;
  description: string | null;
  status: "DRAFT" | "ACTIVE" | "ARCHIVED";
  current_version_id: string | null;
  created_at?: string;
  updated_at?: string;
}

/** A DST anomaly the preview reports (spec 7.7). */
export interface ScheduleAnomaly {
  at: string;
  kind: "NONEXISTENT_LOCAL_TIME" | "AMBIGUOUS_LOCAL_TIME";
  note: string;
}

export interface SchedulePreview {
  expression: string;
  timezone: string;
  next_run_at: string | null;
  occurrences: string[];
  anomalies: ScheduleAnomaly[];
}

export interface Queue {
  id: string;
  name: string;
  max_concurrency: number | null;
  paused?: boolean;
  /** Executions waiting for a worker (UI.md 27). */
  depth?: number;
  /** Executions currently dispatched or running. */
  running?: number;
  /** When the oldest waiting execution was created, if any. */
  oldest_queued_at?: string | null;
}

export interface ApiKey {
  id: string;
  name: string;
  prefix: string | null;
  revoked: boolean;
  expires_at: string | null;
  created_at: string;
  last_used_at: string | null;
}

export interface AuditEvent {
  id: string;
  actor_type: string;
  actor_id: string | null;
  action: string;
  resource_type: string;
  resource_id: string | null;
  result: "SUCCESS" | "DENIED" | "FAILURE";
  source_ip: string | null;
  request_id: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface UserSummary {
  id: string;
  email: string;
  display_name: string | null;
  role: Role;
  disabled: boolean;
  created_at: string;
}

/** Spec 05 §5.10. */
export type Role = "OWNER" | "ADMIN" | "OPERATOR" | "DEVELOPER" | "AUDITOR" | "VIEWER";

/** Spec 11.4 permissions, used by the UI to hide what the caller cannot do. */
export type Permission =
  | "jobs:read"
  | "jobs:write"
  | "jobs:trigger"
  | "jobs:delete"
  | "job_versions:read"
  | "job_versions:write"
  | "executions:read"
  | "executions:cancel"
  | "executions:retry"
  | "schedules:read"
  | "schedules:write"
  | "queues:read"
  | "queues:write"
  | "workflows:read"
  | "workflows:write"
  | "workflows:trigger"
  | "workers:read"
  | "workers:admin"
  | "audit:read"
  | "users:read"
  | "users:write"
  | "settings:write";

/** Which permissions each role grants, mirroring `forge-auth`. */
const ROLE_PERMISSIONS: Record<Role, Permission[] | ["*"]> = {
  OWNER: ["*"],
  ADMIN: [
    "jobs:read",
    "jobs:write",
    "jobs:trigger",
    "jobs:delete",
    "job_versions:read",
    "job_versions:write",
    "executions:read",
    "executions:cancel",
    "executions:retry",
    "schedules:read",
    "schedules:write",
    "queues:read",
    "queues:write",
    "workflows:read",
    "workflows:write",
    "workflows:trigger",
    "workers:read",
    "workers:admin",
    "users:read",
    "users:write",
    "settings:write",
  ],
  OPERATOR: [
    "jobs:read",
    "jobs:trigger",
    "executions:read",
    "executions:cancel",
    "executions:retry",
    "schedules:read",
    "queues:read",
    "workflows:read",
    "workflows:trigger",
    "workers:read",
    "audit:read",
  ],
  DEVELOPER: [
    "jobs:read",
    "jobs:write",
    "jobs:trigger",
    "job_versions:read",
    "job_versions:write",
    "executions:read",
    "executions:cancel",
    "schedules:read",
    "schedules:write",
    "queues:read",
    "workflows:read",
    "workflows:write",
    "workflows:trigger",
    "workers:read",
  ],
  AUDITOR: [
    "jobs:read",
    "job_versions:read",
    "executions:read",
    "schedules:read",
    "queues:read",
    "workflows:read",
    "workers:read",
    "audit:read",
  ],
  VIEWER: [
    "jobs:read",
    "executions:read",
    "schedules:read",
    "queues:read",
    "workflows:read",
    "workers:read",
  ],
};

/** Whether a role grants a permission. */
export function roleCan(role: Role | null | undefined, permission: Permission): boolean {
  if (!role) return false;
  const granted = ROLE_PERMISSIONS[role];
  return granted.some((p) => p === "*" || p === permission);
}

/** Whether an execution is in a state that will not change on its own. */
export function isTerminal(status: ExecutionStatus): boolean {
  return (
    status === "SUCCEEDED" ||
    status === "FAILED" ||
    status === "CANCELLED" ||
    status === "TIMED_OUT" ||
    status === "DEAD_LETTERED"
  );
}

/** Whether an execution still occupies a concurrency slot. */
export function holdsSlot(status: ExecutionStatus): boolean {
  return !isTerminal(status);
}

/** A short label for a status, for tables and badges. */
export const STATUS_LABELS: Record<string, string> = {
  SCHEDULED: "Scheduled",
  QUEUED: "Queued",
  DISPATCHED: "Dispatched",
  RUNNING: "Running",
  SUCCEEDED: "Succeeded",
  FAILED: "Failed",
  TIMED_OUT: "Timed out",
  CANCEL_REQUESTED: "Cancelling",
  CANCELLED: "Cancelled",
  RETRY_SCHEDULED: "Retry scheduled",
  DEAD_LETTERED: "Dead lettered",
  ABANDONED: "Abandoned",
  DRAFT: "Draft",
  ACTIVE: "Active",
  ARCHIVED: "Archived",
  READY: "Ready",
  BUSY: "Busy",
  DRAINING: "Draining",
  OFFLINE: "Offline",
  REVOKED: "Revoked",
  REGISTERING: "Registering",
};

/** Formats an instant with its timezone, which spec 7.2 requires. */
export function formatTimestamp(value: string | null | undefined): string {
  if (!value) return "-";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return "-";
  return `${parsed.toISOString().replace("T", " ").slice(0, 19)} UTC`;
}

/** Formats an instant as a relative age, e.g. "3m ago". */
export function formatRelative(value: string | null | undefined): string {
  if (!value) return "-";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return "-";

  const seconds = Math.round((Date.now() - parsed.getTime()) / 1000);
  const future = seconds < 0;
  const magnitude = Math.abs(seconds);

  const units: [number, string][] = [
    [60, "s"],
    [3600, "m"],
    [86400, "h"],
    [2592000, "d"],
  ];

  let amount = magnitude;
  let label = "s";
  for (const [limit, unit] of units) {
    if (magnitude < limit) {
      amount = magnitude;
      label = unit;
      break;
    }
    amount = Math.round(magnitude / (limit === 60 ? 60 : limit / 60));
    label = unit;
  }
  return future ? `in ${amount}${label}` : `${amount}${label} ago`;
}

/** Formats a duration in milliseconds, e.g. "1.2s". */
export function formatDuration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "-";
  if (ms < 1000) return `${Math.round(ms)}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `${minutes}m ${seconds}s`;
}

export interface Team {
  id: string;
  name: string;
  description: string | null;
  on_call: string | null;
  members: number;
  created_at: string;
}

