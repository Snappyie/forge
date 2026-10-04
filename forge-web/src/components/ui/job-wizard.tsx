"use client";

import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { useToast } from "@/lib/useToast";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
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
import { useUnsavedChanges } from "@/lib/useUnsavedChanges";

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
}

const EMPTY: Draft = {
  name: "",
  key: "",
  description: "",
  priority: "NORMAL",
  cron: "0 2 * * *",
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
};

export function JobWizard() {
  const toast = useToast();
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [busy, setBusy] = useState(false);
  const [workflows, setWorkflows] = useState<{ id: string, name: string }[]>([]);
  const [webhooks, setWebhooks] = useState<{ id: string, name: string, url: string }[]>([]);
  const [teams, setTeams] = useState<{ id: string, name: string, email: string }[]>([]);
  const [schedules, setSchedules] = useState<{ id: string, cron: string, name: string }[]>([]);
  const [queues, setQueues] = useState<{ id: string, name: string }[]>([]);

  useEffect(() => {
    api.get<{ id: string, name: string }[]>("/workflows")
      .then(res => setWorkflows(Array.isArray(res) ? res : []))
      .catch(() => {});
    api.get<{ id: string, name: string, url: string }[]>("/webhooks")
      .then(res => setWebhooks(Array.isArray(res) ? res : []))
      .catch(() => {});
    api.get<{ id: string, name: string, email: string }[]>("/teams")
      .then(res => setTeams(Array.isArray(res) ? res : []))
      .catch(() => {});
    api.get<{ id: string, cron: string, name: string }[]>("/schedules")
      .then(res => setSchedules(Array.isArray(res) ? res : []))
      .catch(() => {});
    api.get<{ id: string, name: string }[]>("/queues")
      .then(res => setQueues(Array.isArray(res) ? res : []))
      .catch(() => {});
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

      // Create version
      const version = await api.post<{ id: string }>(`/jobs/${jobId}/versions`, {
        config,
        execution_type: draft.executionType,
        timeout_seconds: parseInt(draft.timeoutSeconds) || 3600,
        concurrency_policy: { scope: "JOB", max_concurrent_executions: draft.concurrencyPolicy === "ALLOW" ? null : { "Bounded": 1 } },
        retry_policy: { max_attempts: parseInt(draft.maxAttempts) || 3 }
      });
      
      // Publish version
      await api.post(`/jobs/${jobId}/versions/${version.id}/publish`, {});
      toast.success("Job successfully published!");
      
      // Clear draft and let them go to jobs list (in a real app, we'd navigate to /jobs)
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

  async function attachSchedule() {
    const jobId = draft.jobId ?? (await saveDraft());
    if (!jobId) return;
    setBusy(true);
    try {
      await api.post("/schedules", {
        target_id: jobId,
        expression: draft.cron,
        timezone: draft.timezone || "UTC",
      });
      toast.success("Schedule attached");
    } catch (error) {
      toast.error(
        "Could not attach schedule",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card className="max-w-3xl mx-auto w-full mb-10 border shadow-sm">
      <CardHeader>
        <CardTitle className="text-xl">
          {draft.jobId ? "Editing Job Draft" : "Create New Job"}
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-8 h-full pr-4">
        
        {/* BASICS */}
        <div className="flex flex-col gap-4 pb-6 border-b border-border">
          <h3 className="font-medium text-lg">1. Basic Information</h3>
          <div className="grid grid-cols-2 gap-4">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="w-name">Name</Label>
              <Input
                id="w-name"
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="w-key">Key (Optional)</Label>
              <Input
                id="w-key"
                value={draft.key}
                onChange={(e) => setDraft({ ...draft, key: e.target.value })}
                placeholder="nightly-settlement"
              />
            </div>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="w-desc">Description</Label>
            <Textarea
              id="w-desc"
              rows={3}
              value={draft.description}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
            />
          </div>
        </div>

        {/* EXECUTION */}
        <div className="flex flex-col gap-4 pb-6 border-b border-border">
          <h3 className="font-medium text-lg">2. Execution Policy</h3>
          <div className="grid grid-cols-2 gap-4">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="w-execution-type">Execution Type</Label>
              <Select value={draft.executionType} onValueChange={(value) => setDraft({ ...draft, executionType: value ?? "WORKER_TASK" })}>
                <SelectTrigger id="w-execution-type"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="CONTAINER_COMMAND">Shell Command</SelectItem>
                  <SelectItem value="HTTP_REQUEST">HTTP Request</SelectItem>
                  <SelectItem value="WORKER_TASK">Queue Worker</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {draft.executionType === "WORKER_TASK" && (
              <div className="flex flex-col gap-1.5 relative">
                <Label htmlFor="w-queue-id">Target Queue</Label>
                <Input
                  id="w-queue-id"
                  list="queues-list"
                  value={queues.find(q => q.id === draft.queueId)?.name || draft.queueId}
                  onChange={(e) => {
                    const match = queues.find(q => q.name === e.target.value);
                    setDraft({ ...draft, queueId: match ? match.id : e.target.value });
                  }}
                  placeholder="ecommerce-queue"
                  autoComplete="off"
                />
                <datalist id="queues-list">
                  {queues.map(q => (
                    <option key={q.id} value={q.name} />
                  ))}
                </datalist>
              </div>
            )}
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="w-concurrency">Concurrency Policy</Label>
              <Select value={draft.concurrencyPolicy} onValueChange={(value) => setDraft({ ...draft, concurrencyPolicy: value ?? "ALLOW" })}>
                <SelectTrigger id="w-concurrency"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="ALLOW">Allow</SelectItem>
                  <SelectItem value="FORBID">Forbid (Skip)</SelectItem>
                  <SelectItem value="REPLACE">Replace (Cancel running)</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="w-priority">Priority</Label>
              <Select value={draft.priority} onValueChange={(value) => setDraft({ ...draft, priority: value ?? "NORMAL" })}>
                <SelectTrigger id="w-priority"><SelectValue /></SelectTrigger>
                <SelectContent>
                  {["CRITICAL", "HIGH", "NORMAL", "LOW", "BACKGROUND"].map((p) => (
                    <SelectItem key={p} value={p}>{p}</SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="w-timeout">Timeout (seconds)</Label>
              <Input
                id="w-timeout"
                type="number"
                value={draft.timeoutSeconds}
                onChange={(e) => setDraft({ ...draft, timeoutSeconds: e.target.value })}
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="w-attempts">Max attempts</Label>
              <Input
                id="w-attempts"
                type="number"
                value={draft.maxAttempts}
                onChange={(e) => setDraft({ ...draft, maxAttempts: e.target.value })}
              />
            </div>
          </div>
        </div>

        {/* SCHEDULE */}
        <div className="flex flex-col gap-4 pb-6 border-b border-border">
          <div className="flex items-center justify-between">
             <h3 className="font-medium text-lg">3. Schedule</h3>
             <Button variant="outline" size="sm" onClick={attachSchedule} disabled={busy || !draft.cron}>
              Attach Schedule
            </Button>
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="w-cron">Cron expression (Search saved schedules)</Label>
              <Input
                id="w-cron"
                list="schedules-list"
                value={draft.cron}
                onChange={(e) => setDraft({ ...draft, cron: e.target.value })}
                className="font-mono"
                placeholder="*/15 * * * *"
                autoComplete="off"
              />
              <datalist id="schedules-list">
                {schedules.map(sch => (
                  <option key={sch.id} value={sch.cron}>{sch.name}</option>
                ))}
              </datalist>
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="w-tz">Timezone</Label>
              <Input
                id="w-tz"
                value={draft.timezone}
                onChange={(e) => setDraft({ ...draft, timezone: e.target.value })}
              />
            </div>
          </div>
        </div>

        {/* ALERTS */}
        <div className="flex flex-col gap-4 pb-2">
          <h3 className="font-medium text-lg">4. Alerts & Runbook</h3>
          <div className="grid grid-cols-2 gap-4">
            <div className="flex flex-col gap-1.5 relative">
              <Label htmlFor="w-alert-email">Alert Email (Search Teams)</Label>
              <Input
                id="w-alert-email"
                type="email"
                list="teams-list"
                value={draft.alertEmail}
                onChange={(e) => setDraft({ ...draft, alertEmail: e.target.value })}
                placeholder="oncall@example.com"
                autoComplete="off"
              />
              <datalist id="teams-list">
                {teams.map(t => (
                  <option key={t.id} value={t.email}>{t.name}</option>
                ))}
              </datalist>
            </div>
            <div className="flex flex-col gap-1.5 relative">
              <Label htmlFor="w-alert-webhook">Alert Webhook URL (Search Webhooks)</Label>
              <Input
                id="w-alert-webhook"
                type="url"
                list="webhooks-list"
                value={draft.alertWebhook}
                onChange={(e) => setDraft({ ...draft, alertWebhook: e.target.value })}
                placeholder="https://hooks.slack.com/services/..."
                autoComplete="off"
              />
              <datalist id="webhooks-list">
                {webhooks.map(wh => (
                  <option key={wh.id} value={wh.url}>{wh.name}</option>
                ))}
              </datalist>
            </div>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="w-runbook">Runbook URL</Label>
            <Input
              id="w-runbook"
              value={draft.runbook}
              onChange={(e) => setDraft({ ...draft, runbook: e.target.value })}
              placeholder="https://wiki.example.com/runbooks/settlement"
            />
          </div>
        </div>

        <div className="flex items-center gap-2 mt-4 pt-4 border-t border-border">
          <Button
            size="lg"
            variant="outline"
            onClick={saveDraft}
            disabled={busy || draft.name.trim() === ""}
            className="w-1/3 font-medium"
          >
            {busy ? "Saving…" : draft.jobId ? "Update Draft" : "Save Draft"}
          </Button>
          <Button
            size="lg"
            onClick={publishJob}
            disabled={busy || draft.name.trim() === ""}
            className="w-2/3 font-medium"
          >
            Publish Job & Activate
          </Button>
          {draft.jobId ? (
            <Badge variant="secondary" className="ml-auto text-xs shrink-0 py-1 px-2">
              Draft ID: {draft.jobId.slice(0, 8)}
            </Badge>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}
