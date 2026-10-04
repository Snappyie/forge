"use client";

import { useEffect, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { ChevronLeft, Check, Copy, Info } from "lucide-react";

import { api } from "@/lib/api";
import { useToast } from "@/lib/useToast";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import {
  SchedulePreview,
  describeCron,
} from "@/components/ui/schedule-preview";
import { useUnsavedChanges } from "@/lib/useUnsavedChanges";
import { cn } from "cn";

const DRAFT_KEY = "forge.jobWizard.draft";

interface Draft {
  jobId?: string;
  name: string;
  key: string;
  description: string;
  priority: string;
  cron: string;
  timezone: string;
  timeoutSeconds: string;
  maxAttempts: string;
  runbook: string;
  executionType: string;
  linkedWorkflowId: string;
  queueId: string;
  alertEmail: string;
  alertWebhook: string;
  concurrencyPolicy: string;
  /** Misfire policy: what happens to an occurrence missed while Forge was down. */
  misfirePolicy: "FIRE_ONCE" | "CATCH_UP" | "SKIP";
  /** True when this job is only ever triggered by hand and has no schedule. */
  manualOnly: boolean;
}

const EMPTY: Draft = {
  name: "",
  key: "",
  description: "",
  priority: "NORMAL",
  cron: "0 2 * * 1-5",
  timezone: "UTC",
  timeoutSeconds: "3600",
  maxAttempts: "3",
  runbook: "",
  executionType: "WORKER_TASK",
  linkedWorkflowId: "",
  queueId: "",
  alertEmail: "",
  alertWebhook: "",
  concurrencyPolicy: "ALLOW",
  misfirePolicy: "FIRE_ONCE",
  manualOnly: false,
};

/**
 * The patterns the builder offers.
 *
 * Each entry is a real five-field cron with the minute and hour slots marked,
 * so choosing one fills the expression field and the preview immediately shows
 * what it means. Offering an "every N minutes" preset with an hour of `*` would
 * be honest but useless, so intervals are left to the expression field.
 */
const PATTERNS: { id: string; label: string; cron: string }[] = [
  { id: "daily", label: "Every day at a specific time", cron: "0 2 * * *" },
  { id: "weekday", label: "Every weekday at a specific time", cron: "0 2 * * 1-5" },
  { id: "weekly", label: "Weekly on specific days", cron: "0 2 * * 1" },
  { id: "hourly", label: "Every hour", cron: "0 * * * *" },
  { id: "interval", label: "Every 15 minutes", cron: "*/15 * * * *" },
  { id: "custom", label: "Custom expression", cron: "" },
];

/**
 * Replaces the minute and hour fields of a cron expression, leaving the other
 * three alone. Returns the expression unchanged if it is not five fields, so a
 * half-typed expression is not silently rewritten.
 */
function applyTime(expression: string, time: string): string {
  const fields = expression.trim().split(/\s+/);
  const [hour, minute] = time.split(":");
  if (fields.length !== 5 || !hour || !minute) return expression;
  return [minute.padStart(2, "0"), hour.padStart(2, "0"), ...fields.slice(2)].join(" ");
}

/** The timezones offered in the picker. Any IANA name is accepted on save. */
const TIMEZONES = [
  "UTC",
  "Asia/Kolkata",
  "Asia/Singapore",
  "Europe/London",
  "Europe/Berlin",
  "America/New_York",
  "America/Los_Angeles",
  "Australia/Sydney",
];

const TABS = [
  { id: 1, label: "Basics" },
  { id: 2, label: "Schedule" },
  { id: 3, label: "Execution" },
  { id: 4, label: "Retry" },
  { id: 5, label: "Alerts" },
  { id: 6, label: "Review" },
];

export function JobWizard() {
  const router = useRouter();
  const toast = useToast();
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [activeTab, setActiveTab] = useState(1);
  const [busy, setBusy] = useState(false);
  const [workflows, setWorkflows] = useState<{ id: string; name: string }[]>([]);
  const [webhooks, setWebhooks] = useState<{ id: string; name: string; url: string }[]>([]);
  const [teams, setTeams] = useState<{ id: string; name: string; email: string }[]>([]);
  const [schedules, setSchedules] = useState<{ id: string; cron: string; name: string }[]>([]);
  const [queues, setQueues] = useState<{ id: string; name: string }[]>([]);

  useEffect(() => {
    api.get<{ id: string; name: string }[]>("/workflows").then(res => setWorkflows(Array.isArray(res) ? res : [])).catch(() => {});
    api.get<{ id: string; name: string; url: string }[]>("/webhooks").then(res => setWebhooks(Array.isArray(res) ? res : [])).catch(() => {});
    api.get<{ id: string; name: string; email: string }[]>("/teams").then(res => setTeams(Array.isArray(res) ? res : [])).catch(() => {});
    api.get<{ id: string; cron: string; name: string }[]>("/schedules").then(res => setSchedules(Array.isArray(res) ? res : [])).catch(() => {});
    api.get<{ id: string; name: string }[]>("/queues").then(res => setQueues(Array.isArray(res) ? res : [])).catch(() => {});
  }, []);

  useEffect(() => {
    try {
      const raw = window.localStorage.getItem(DRAFT_KEY);
      if (raw) setDraft({ ...EMPTY, ...JSON.parse(raw) });
    } catch {
      setDraft(EMPTY);
    }
  }, []);

  useEffect(() => {
    if (draft.name.trim() === "") return;
    try {
      window.localStorage.setItem(DRAFT_KEY, JSON.stringify(draft));
    } catch {}
  }, [draft]);

  const dirty = draft.name.trim() !== "";
  useUnsavedChanges(dirty);

  // The pattern picker and the time input are views onto the cron expression
  // rather than state of their own. Deriving them keeps one source of truth, so
  // typing an expression by hand cannot leave the picker claiming something the
  // expression does not actually say.
  const patternId = useMemo(() => {
    const match = PATTERNS.find((p) => p.cron && p.cron === draft.cron);
    return match ? match.id : "custom";
  }, [draft.cron]);

  const cronTime = useMemo(() => {
    const fields = draft.cron.trim().split(/\s+/);
    if (fields.length !== 5) return "";
    const [, hour, minute] = fields;
    if (!/^\d+$/.test(hour) || !/^\d+$/.test(minute)) return "";
    return `${hour.padStart(2, "0")}:${minute.padStart(2, "0")}`;
  }, [draft.cron]);

  async function saveDraft(): Promise<string | undefined> {
    if (!draft.name.trim()) {
      toast.error("Job name is required");
      return undefined;
    }
    setBusy(true);
    try {
      if (draft.jobId) {
        await api.patch(`/jobs/${draft.jobId}`, {
          name: draft.name,
          key: draft.key || undefined,
          description: draft.description,
          priority: draft.priority,
        });
        toast.success("Draft updated");
        return draft.jobId;
      }
      const created = await api.post<{ id: string }>("/jobs", {
        name: draft.name,
        key: draft.key || undefined,
        description: draft.description,
        priority: draft.priority,
      });
      setDraft((current) => ({ ...current, jobId: created.id }));
      toast.success("Draft saved");
      return created.id;
    } catch (error) {
      toast.error(
        "Could not save draft",
        error instanceof Error ? error.message : undefined,
      );
      return undefined;
    } finally {
      setBusy(false);
    }
  }

  async function publishJob() {
    let jobId = draft.jobId;
    if (!jobId) {
      jobId = await saveDraft();
      if (!jobId) return;
    }
    setBusy(true);
    try {
      let config = {};
      if (draft.executionType === "WORKER_TASK") {
        config = { queue_id: draft.queueId };
      } else if (draft.executionType === "HTTP_REQUEST") {
        config = { url: "https://example.com" }; // Placeholder
      }

      const version = await api.post<{ id: string }>(`/jobs/${jobId}/versions`, {
        config,
        execution_type: draft.executionType,
        timeout_seconds: parseInt(draft.timeoutSeconds) || 3600,
        concurrency_policy: { scope: "JOB", max_concurrent_executions: draft.concurrencyPolicy === "ALLOW" ? null : { Bounded: 1 } },
        retry_policy: { max_attempts: parseInt(draft.maxAttempts) || 3 },
      });
      
      await api.post(`/jobs/${jobId}/versions/${version.id}/publish`, {});

      // A job marked manual-only gets no schedule at all: every execution then
      // comes from a person or the API, which is what the author asked for.
      if (draft.cron.trim() && !draft.manualOnly) {
        await api.post("/schedules", {
          target_type: "JOB",
          target_id: jobId,
          schedule_type: "CRON",
          expression: draft.cron.trim(),
          timezone: draft.timezone || "UTC",
          misfire_policy: draft.misfirePolicy,
        });
      }

      toast.success("Job successfully published!");
      window.localStorage.removeItem(DRAFT_KEY);
      setDraft(EMPTY);
      window.location.href = "/jobs";
    } catch (error) {
      toast.error(
        "Could not publish job",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex w-full items-start">
      {/* ─── Left Sidebar ─── */}
      <div className="sticky top-0 flex w-64 shrink-0 flex-col self-start border-r border-border bg-background">
        <div className="p-4 pt-6">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => router.push("/jobs")}
            className="mb-4 h-8 px-2 text-muted-foreground hover:text-foreground"
          >
            <ChevronLeft className="mr-1 size-4" />
            New job
          </Button>
          <p className="px-2 text-xs text-muted-foreground">Saved as a draft</p>
        </div>

        <nav className="flex-1 space-y-1 p-2">
          {TABS.map((tab) => {
            const isActive = activeTab === tab.id;
            const isCompleted = activeTab > tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={cn(
                  "flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                  isActive
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent/50 hover:text-foreground"
                )}
              >
                <span
                  className={cn(
                    "flex size-5 shrink-0 items-center justify-center rounded-full text-[10px]",
                    isCompleted
                      ? "bg-emerald-500 text-white"
                      : isActive
                      ? "bg-foreground text-background"
                      : "bg-muted text-muted-foreground"
                  )}
                >
                  {isCompleted ? <Check className="size-3" /> : tab.id}
                </span>
                {tab.label}
              </button>
            );
          })}
        </nav>

        <div className="p-4 pb-6 space-y-4">
          <p className="text-xs text-muted-foreground leading-relaxed">
            A draft never runs. Publishing a version is what makes this job
            eligible for dispatch.
          </p>
          <div className="flex items-start gap-2 rounded-md border border-border bg-muted/30 p-3 text-xs text-muted-foreground">
            <Info className="mt-0.5 size-4 shrink-0" />
            <p>
              Draft mirrored to this browser.
              {draft.jobId && (
                <span className="block mt-1">
                  Server draft <code>{draft.jobId.slice(0, 8)}</code>...
                </span>
              )}
            </p>
          </div>
        </div>
      </div>

      {/* ─── Main Content ─── */}
      <div className="min-w-0 flex-1 bg-background p-8 lg:p-12">
        <div className="mx-auto max-w-3xl">
          {activeTab === 1 && (
            <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4">
              <div>
                <h2 className="text-2xl font-semibold tracking-tight">Basic Information</h2>
                <p className="text-sm text-muted-foreground mt-1">What is this job and what does it do?</p>
              </div>
              <div className="grid gap-6">
                <div className="grid gap-2">
                  <Label htmlFor="w-name">Name</Label>
                  <Input
                    id="w-name"
                    value={draft.name}
                    onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                    className="max-w-md"
                  />
                </div>
                <div className="grid gap-2">
                  <Label htmlFor="w-key">Key (Optional)</Label>
                  <Input
                    id="w-key"
                    value={draft.key}
                    onChange={(e) => setDraft({ ...draft, key: e.target.value })}
                    placeholder="nightly-settlement"
                    className="max-w-md"
                  />
                  <p className="text-xs text-muted-foreground">A stable identifier for API requests.</p>
                </div>
                <div className="grid gap-2">
                  <Label htmlFor="w-desc">Description</Label>
                  <Textarea
                    id="w-desc"
                    rows={4}
                    value={draft.description}
                    onChange={(e) => setDraft({ ...draft, description: e.target.value })}
                  />
                </div>
              </div>
            </div>
          )}

{activeTab === 2 && (
            <div className="space-y-6 animate-in fade-in slide-in-from-bottom-4 max-w-3xl">
              <div>
                <h2 className="text-lg font-semibold tracking-tight">When should this run?</h2>
                <p className="text-xs text-muted-foreground mt-1">
                  Pick a pattern or type an expression. The preview below is computed by the same
                  engine that fires the job, so it cannot disagree with what actually runs.
                </p>
              </div>

              <div className="flex items-start gap-2 rounded-lg border border-border bg-muted/30 p-3">
                <span className="mt-0.5">
                  <Info className="size-3.5 text-muted-foreground" aria-hidden />
                </span>
                <div className="flex-1">
                  <label
                    htmlFor="w-manual-only"
                    className="flex cursor-pointer items-start gap-2 text-xs font-medium"
                  >
                    <input
                      id="w-manual-only"
                      type="checkbox"
                      checked={draft.manualOnly}
                      onChange={(e) => setDraft({ ...draft, manualOnly: e.target.checked })}
                      className="mt-0.5 size-3.5 accent-primary"
                    />
                    This job is only triggered by hand
                  </label>
                  <p className="text-[11px] text-muted-foreground mt-1 ml-5">
                    {draft.manualOnly
                      ? "No schedule will be created. Trigger it from the job page or the API."
                      : "Leave this off and Forge will create a schedule from the expression below."}
                  </p>
                </div>
              </div>

              {!draft.manualOnly ? (
                <>
                  <div className="grid gap-4 sm:grid-cols-2">
                    <div className="grid gap-2">
                      <Label htmlFor="w-pattern">Pattern</Label>
                      <Select
                        value={patternId}
                        onValueChange={(value) => {
                          const pattern = PATTERNS.find((p) => p.id === value);
                          // "Custom" leaves the expression alone so typing is not
                          // overwritten; every other preset fills it in.
                          if (pattern && pattern.cron) {
                            setDraft({ ...draft, cron: pattern.cron });
                          }
                        }}
                      >
                        <SelectTrigger id="w-pattern" className="h-auto min-h-8 items-start py-1.5 text-left">
                          <SelectValue>
                            {() =>
                              PATTERNS.find((p) => p.id === patternId)?.label ?? "Custom expression"
                            }
                          </SelectValue>
                        </SelectTrigger>
                        <SelectContent>
                          {PATTERNS.map((pattern) => (
                            <SelectItem key={pattern.id} value={pattern.id}>
                              {pattern.label}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    </div>

                    <div className="grid gap-2">
                      <Label htmlFor="w-time">Time (24h)</Label>
                      <Input
                        id="w-time"
                        type="time"
                        step={60}
                        value={cronTime}
                        onChange={(e) => setDraft({ ...draft, cron: applyTime(draft.cron, e.target.value) })}
                      />
                      <p className="text-[11px] text-muted-foreground">
                        Sets the minute and hour fields, leaving the rest untouched.
                      </p>
                    </div>
                  </div>

                  <div className="grid gap-4 sm:grid-cols-2">
                    <div className="grid gap-2">
                      <Label htmlFor="w-timezone">Timezone</Label>
                      <Select
                        value={draft.timezone}
                        onValueChange={(value) => setDraft({ ...draft, timezone: value ?? "UTC" })}
                      >
                        <SelectTrigger id="w-timezone" className="whitespace-normal">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          {TIMEZONES.map((zone) => (
                            <SelectItem key={zone} value={zone}>
                              {zone}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                      <p className="text-[11px] text-muted-foreground">
                        Never inferred from the server. A missing timezone is a validation error.
                      </p>
                    </div>

                    <div className="grid gap-2">
                      <Label htmlFor="w-cron">Cron expression</Label>
                      <div className="flex gap-2">
                        <Input
                          id="w-cron"
                          value={draft.cron}
                          onChange={(e) =>
                            setDraft({ ...draft, cron: e.target.value })
                          }
                          className="font-mono"
                        />
                        <Button
                          type="button"
                          variant="outline"
                          size="icon"
                          aria-label="Copy cron expression"
                          onClick={() => {
                            navigator.clipboard?.writeText(draft.cron);
                            toast.success("Cron expression copied");
                          }}
                        >
                          <Copy className="size-4" aria-hidden />
                        </Button>
                      </div>
                      <p className="text-[11px] text-muted-foreground">
                        {describeCron(draft.cron) ? (
                          <>
                            <span className="font-medium text-foreground">
                              {describeCron(draft.cron)}.
                            </span>{" "}
                            Five fields, weekday numbering from Sunday.
                          </>
                        ) : (
                          "Five fields: minute hour day-of-month month day-of-week."
                        )}
                      </p>
                    </div>
                  </div>

                  <SchedulePreview expression={draft.cron} timezone={draft.timezone} count={6} />

                  <div className="rounded-lg border border-border">
                    <div className="bg-muted/30 p-4 border-b border-border">
                      <p className="text-sm font-medium">
                        If Forge was down when this was due, what should happen?
                      </p>
                      <p className="text-xs text-muted-foreground mt-1">
                        This is the misfire policy, and it is the difference between a missed run and
                        a stampede.
                      </p>
                    </div>
                    <div className="p-4">
                      <RadioGroup
                        value={draft.misfirePolicy}
                        onValueChange={(value) =>
                          setDraft({
                            ...draft,
                            misfirePolicy: (value ?? "FIRE_ONCE") as Draft["misfirePolicy"],
                          })
                        }
                      >
                        <label
                          htmlFor="run_once"
                          className={cn(
                            "flex items-start space-x-3 rounded-md border p-3 mb-3 cursor-pointer",
                            draft.misfirePolicy === "FIRE_ONCE"
                              ? "border-primary bg-background"
                              : "border-transparent hover:bg-muted/30",
                          )}
                        >
                          <RadioGroupItem value="FIRE_ONCE" id="run_once" className="mt-1" />
                          <div>
                            <span className="text-sm font-medium block">Run once, late</span>
                            <span className="text-xs text-muted-foreground">
                              One execution fires immediately when Forge recovers. Missed
                              occurrences are skipped, not queued.{" "}
                              <span className="font-medium text-foreground">Default.</span>
                            </span>
                          </div>
                        </label>

                        <label
                          htmlFor="catch_up"
                          className={cn(
                            "flex items-start space-x-3 rounded-md border border-transparent p-3 mb-3 cursor-pointer",
                            draft.misfirePolicy === "CATCH_UP"
                              ? "border-primary bg-background"
                              : "hover:bg-muted/30",
                          )}
                        >
                          <RadioGroupItem value="CATCH_UP" id="catch_up" className="mt-1" />
                          <div>
                            <span className="text-sm font-medium block">
                              Catch up every missed run
                            </span>
                            <span className="text-xs text-muted-foreground">
                              Replays each missed occurrence, oldest first, up to a limit of 100.
                              Safe for idempotent jobs; dangerous for ones that charge a card.
                            </span>
                          </div>
                        </label>

                        <label
                          htmlFor="skip"
                          className={cn(
                            "flex items-start space-x-3 rounded-md border border-transparent p-3 cursor-pointer",
                            draft.misfirePolicy === "SKIP"
                              ? "border-primary bg-background"
                              : "hover:bg-muted/30",
                          )}
                        >
                          <RadioGroupItem value="SKIP" id="skip" className="mt-1" />
                          <div>
                            <span className="text-sm font-medium block">Skip entirely</span>
                            <span className="text-xs text-muted-foreground">
                              The occurrence is discarded. Use when a late run is worse than no run.
                            </span>
                          </div>
                        </label>
                      </RadioGroup>
                    </div>
                  </div>
                </>
              ) : null}
            </div>
          )}

          {activeTab === 3 && (
            <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4">
              <div>
                <h2 className="text-2xl font-semibold tracking-tight">Execution</h2>
                <p className="text-sm text-muted-foreground mt-1">How should Forge run this job?</p>
              </div>
              <div className="grid gap-6 max-w-2xl">
                <div className="grid gap-2">
                  <Label>Execution Type</Label>
                  <Select value={draft.executionType} onValueChange={(val) => setDraft({ ...draft, executionType: val ?? "WORKER_TASK" })}>
                    <SelectTrigger><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="WORKER_TASK">Queue Worker</SelectItem>
                      <SelectItem value="CONTAINER_COMMAND">Shell Command</SelectItem>
                      <SelectItem value="HTTP_REQUEST">HTTP Request</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                {draft.executionType === "WORKER_TASK" && (
                  <div className="grid gap-2 relative">
                    <Label>Target Queue</Label>
                    <Input
                      list="queues-list"
                      value={queues.find((q) => q.id === draft.queueId)?.name || draft.queueId}
                      onChange={(e) => {
                        const match = queues.find((q) => q.name === e.target.value);
                        setDraft({ ...draft, queueId: match ? match.id : e.target.value });
                      }}
                      placeholder="ecommerce-queue"
                    />
                    <datalist id="queues-list">
                      {queues.map((q) => (
                        <option key={q.id} value={q.name} />
                      ))}
                    </datalist>
                  </div>
                )}
                <div className="grid gap-2">
                  <Label>Priority</Label>
                  <Select value={draft.priority} onValueChange={(val) => setDraft({ ...draft, priority: val ?? "NORMAL" })}>
                    <SelectTrigger><SelectValue /></SelectTrigger>
                    <SelectContent>
                      {["CRITICAL", "HIGH", "NORMAL", "LOW", "BACKGROUND"].map((p) => (
                        <SelectItem key={p} value={p}>{p}</SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="grid gap-2">
                  <Label>Timeout (seconds)</Label>
                  <Input
                    type="number"
                    value={draft.timeoutSeconds}
                    onChange={(e) => setDraft({ ...draft, timeoutSeconds: e.target.value })}
                  />
                  <p className="text-xs text-muted-foreground">Kill the execution if it takes longer than this.</p>
                </div>
                <div className="grid gap-2">
                  <Label>Concurrency Policy</Label>
                  <Select value={draft.concurrencyPolicy} onValueChange={(val) => setDraft({ ...draft, concurrencyPolicy: val ?? "ALLOW" })}>
                    <SelectTrigger><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="ALLOW">Allow (Run concurrently)</SelectItem>
                      <SelectItem value="FORBID">Forbid (Skip if already running)</SelectItem>
                      <SelectItem value="REPLACE">Replace (Cancel running execution)</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>
            </div>
          )}

          {activeTab === 4 && (
            <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4">
              <div>
                <h2 className="text-2xl font-semibold tracking-tight">Retry Policy</h2>
                <p className="text-sm text-muted-foreground mt-1">What to do when the job fails.</p>
              </div>
              <div className="grid gap-6 max-w-2xl">
                <div className="grid gap-2">
                  <Label>Maximum attempts</Label>
                  <Input
                    type="number"
                    value={draft.maxAttempts}
                    onChange={(e) => setDraft({ ...draft, maxAttempts: e.target.value })}
                  />
                  <p className="text-xs text-muted-foreground">The total number of tries (including the first attempt). E.g. 3 means 2 retries.</p>
                </div>
              </div>
            </div>
          )}

          {activeTab === 5 && (
            <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4">
              <div>
                <h2 className="text-2xl font-semibold tracking-tight">Alerts & Runbook</h2>
                <p className="text-sm text-muted-foreground mt-1">Who to notify and what to read during an incident.</p>
              </div>
              <div className="grid gap-6 max-w-2xl">
                <div className="grid gap-2">
                  <Label>Alert Email</Label>
                  <Input
                    type="email"
                    list="teams-list"
                    value={draft.alertEmail}
                    onChange={(e) => setDraft({ ...draft, alertEmail: e.target.value })}
                    placeholder="oncall@example.com"
                  />
                </div>
                <div className="grid gap-2">
                  <Label>Alert Webhook URL</Label>
                  <Input
                    type="url"
                    list="webhooks-list"
                    value={draft.alertWebhook}
                    onChange={(e) => setDraft({ ...draft, alertWebhook: e.target.value })}
                    placeholder="https://hooks.slack.com/services/..."
                  />
                </div>
                <div className="grid gap-2">
                  <Label>Runbook URL</Label>
                  <Input
                    type="url"
                    value={draft.runbook}
                    onChange={(e) => setDraft({ ...draft, runbook: e.target.value })}
                    placeholder="https://wiki.example.com/runbooks/settlement"
                  />
                </div>
              </div>
            </div>
          )}

          {activeTab === 6 && (
            <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4">
              <div>
                <h2 className="text-2xl font-semibold tracking-tight">Review & Publish</h2>
                <p className="text-sm text-muted-foreground mt-1">Double check your job configuration.</p>
              </div>
              
              <div className="rounded-lg border border-border">
                <div className="grid grid-cols-2 gap-4 p-4 border-b border-border">
                  <div>
                    <p className="text-xs text-muted-foreground">Name</p>
                    <p className="text-sm font-medium">{draft.name || "—"}</p>
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground">Key</p>
                    <p className="text-sm font-medium">{draft.key || "—"}</p>
                  </div>
                </div>
                <div className="grid grid-cols-3 gap-4 p-4 border-b border-border bg-muted/20">
                  <div>
                    <p className="text-xs text-muted-foreground">Execution</p>
                    <p className="text-sm font-medium">{draft.executionType}</p>
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground">Queue</p>
                    <p className="text-sm font-medium">{draft.queueId || "—"}</p>
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground">Priority</p>
                    <p className="text-sm font-medium">{draft.priority}</p>
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-4 p-4">
                  <div>
                    <p className="text-xs text-muted-foreground">Schedule</p>
                    <p className="text-sm font-medium font-mono mt-1">{draft.cron}</p>
                  </div>
                  <div>
                    <p className="text-xs text-muted-foreground">Timeout</p>
                    <p className="text-sm font-medium">{draft.timeoutSeconds}s</p>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Bottom navigation */}
          <div className="mt-12 flex items-center justify-between border-t border-border pt-6 max-w-3xl">
            <Button
              variant="outline"
              onClick={() => setActiveTab(Math.max(1, activeTab - 1))}
              disabled={activeTab === 1}
            >
              Back
            </Button>
            
            {activeTab < 6 ? (
              <div className="flex gap-2">
                <Button
                  variant="secondary"
                  onClick={async () => {
                    await saveDraft();
                  }}
                  disabled={busy || !draft.name.trim()}
                >
                  Save Draft
                </Button>
                <Button
                  onClick={() => setActiveTab(Math.min(6, activeTab + 1))}
                  className="bg-zinc-900 text-white hover:bg-zinc-800"
                >
                  Continue to {TABS[activeTab]?.label?.toLowerCase() || "next"}
                </Button>
              </div>
            ) : (
              <Button
                onClick={publishJob}
                disabled={busy || !draft.name.trim()}
                className="bg-emerald-600 text-white hover:bg-emerald-700"
              >
                {busy ? "Publishing…" : "Publish & Activate"}
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
