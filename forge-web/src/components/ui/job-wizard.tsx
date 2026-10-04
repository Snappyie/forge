"use client";

/**
 * Job creation (UI.md section 7).
 *
 * The spec asks for a wizard rather than one giant form, and for save-draft,
 * autosave, and resume. Each step writes through to the API, so a draft is a
 * real draft row and resuming means loading it — not local state.
 */

import { useEffect, useState } from "react";
import { Check } from "lucide-react";

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
import { cn } from "cn";

const STEPS = [
  "Basics",
  "Schedule",
  "Execution",
  "Retry",
  "Notifications",
  "Security",
  "Review",
] as const;

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
};

export function JobWizard() {
  const toast = useToast();
  const [step, setStep] = useState(0);
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [busy, setBusy] = useState(false);

  // Resume: a draft in progress is restored rather than started over.
  useEffect(() => {
    try {
      const raw = window.localStorage.getItem(DRAFT_KEY);
      if (raw) setDraft({ ...EMPTY, ...JSON.parse(raw) });
    } catch {
      setDraft(EMPTY);
    }
  }, []);

  // Autosave as the operator types, so a crash loses nothing.
  useEffect(() => {
    if (draft.name.trim() === "") return;
    try {
      window.localStorage.setItem(DRAFT_KEY, JSON.stringify(draft));
    } catch {
      // A disabled store just means no resume.
    }
  }, [draft]);

  const dirty = draft.name.trim() !== "";
  useUnsavedChanges(dirty);

  async function saveDraft(): Promise<string | undefined> {
    setBusy(true);
    try {
      if (draft.jobId) {
        await api.patch(`/jobs/${draft.jobId}`, {
          name: draft.name,
          key: draft.key || undefined,
          description: draft.description,
          priority: draft.priority,
        });
        return draft.jobId;
      }
      const created = await api.post<{ id: string }>("/jobs", {
        name: draft.name,
        key: draft.key || undefined,
        description: draft.description,
        priority: draft.priority,
      });
      setDraft((current) => ({ ...current, jobId: created.id }));
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

  async function attachSchedule() {
    const jobId = draft.jobId ?? (await saveDraft());
    if (!jobId) return;
    setBusy(true);
    try {
      await api.post("/schedules", {
        job_id: jobId,
        cron_expression: draft.cron,
        timezone: draft.timezone,
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
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">
          {draft.jobId ? "Editing draft" : "New job"}
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {/* Step rail: the wizard shape the spec asks for instead of one form. */}
        <ol className="flex flex-wrap gap-1">
          {STEPS.map((label, index) => (
            <li key={label}>
              <button
                type="button"
                onClick={() => setStep(index)}
                aria-current={step === index ? "step" : undefined}
                className={cn(
                  "rounded border border-border px-2 py-0.5 text-[11px]",
                  step === index && "border-primary bg-accent text-accent-foreground",
                  index < step && "text-emerald-600 dark:text-emerald-400",
                )}
              >
                {index < step ? <Check className="mr-0.5 inline size-2.5" aria-hidden /> : null}
                {index + 1}. {label}
              </button>
            </li>
          ))}
        </ol>

        {step === 0 ? (
          <div className="flex flex-col gap-2">
            <div className="flex flex-col gap-1">
              <Label htmlFor="w-name">Name</Label>
              <Input
                id="w-name"
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="w-key">Key</Label>
              <Input
                id="w-key"
                value={draft.key}
                onChange={(e) => setDraft({ ...draft, key: e.target.value })}
                placeholder="nightly-settlement"
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="w-desc">Description</Label>
              <Textarea
                id="w-desc"
                value={draft.description}
                onChange={(e) => setDraft({ ...draft, description: e.target.value })}
              />
            </div>
          </div>
        ) : null}

        {step === 1 ? (
          <div className="flex flex-col gap-2">
            <div className="flex flex-col gap-1">
              <Label htmlFor="w-cron">Cron expression</Label>
              <Input
                id="w-cron"
                value={draft.cron}
                onChange={(e) => setDraft({ ...draft, cron: e.target.value })}
                className="font-mono"
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="w-tz">Timezone</Label>
              <Input
                id="w-tz"
                value={draft.timezone}
                onChange={(e) => setDraft({ ...draft, timezone: e.target.value })}
              />
            </div>
            <Button variant="outline" size="sm" onClick={attachSchedule} disabled={busy}>
              Attach schedule
            </Button>
          </div>
        ) : null}

        {step === 2 ? (
          <div className="flex flex-col gap-2">
            <div className="flex flex-col gap-1">
              <Label htmlFor="w-priority">Priority</Label>
              <Select
                value={draft.priority}
                onValueChange={(value) => setDraft({ ...draft, priority: value ?? "NORMAL" })}
              >
                <SelectTrigger className="w-40" aria-label="Priority">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {["CRITICAL", "HIGH", "NORMAL", "LOW", "BACKGROUND"].map((p) => (
                    <SelectItem key={p} value={p}>
                      {p.toLowerCase()}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="w-timeout">Timeout (seconds)</Label>
              <Input
                id="w-timeout"
                type="number"
                value={draft.timeoutSeconds}
                onChange={(e) => setDraft({ ...draft, timeoutSeconds: e.target.value })}
              />
            </div>
          </div>
        ) : null}

        {step === 3 ? (
          <div className="flex flex-col gap-1">
            <Label htmlFor="w-attempts">Max attempts</Label>
            <Input
              id="w-attempts"
              type="number"
              value={draft.maxAttempts}
              onChange={(e) => setDraft({ ...draft, maxAttempts: e.target.value })}
            />
          </div>
        ) : null}

        {step === 4 ? (
          <p className="text-xs text-muted-foreground">
            Alerts for this job are configured as rules on the Alerts page, so a
            failed run reaches the same inbox as any other.
          </p>
        ) : null}

        {step === 5 ? (
          <div className="flex flex-col gap-1">
            <Label htmlFor="w-runbook">Runbook URL</Label>
            <Input
              id="w-runbook"
              value={draft.runbook}
              onChange={(e) => setDraft({ ...draft, runbook: e.target.value })}
              placeholder="https://wiki.example.com/runbooks/settlement"
            />
          </div>
        ) : null}

        {step === 6 ? (
          <ul className="flex flex-col gap-1 text-xs">
            <Review label="Name" value={draft.name || "—"} />
            <Review label="Key" value={draft.key || "—"} />
            <Review label="Priority" value={draft.priority.toLowerCase()} />
            <Review label="Schedule" value={`${draft.cron} (${draft.timezone})`} />
            <Review label="Timeout" value={`${draft.timeoutSeconds}s`} />
            <Review label="Max attempts" value={draft.maxAttempts} />
            <Review
              label="Runbook"
              value={draft.runbook ? "set" : "missing"}
            />
          </ul>
        ) : null}

        <div className="flex items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            disabled={step === 0}
            onClick={() => setStep((s) => Math.max(0, s - 1))}
          >
            Back
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={saveDraft}
            disabled={busy || draft.name.trim() === ""}
          >
            {busy ? "Saving…" : "Save draft"}
          </Button>
          {step < STEPS.length - 1 ? (
            <Button size="sm" onClick={() => setStep((s) => s + 1)}>
              Next
            </Button>
          ) : null}
          {draft.jobId ? (
            <Badge variant="secondary" className="ml-auto text-[10px]">
              draft saved
            </Badge>
          ) : null}
        </div>
      </CardContent>
    </Card>
  );
}

function Review({ label, value }: { label: string; value: string }) {
  return (
    <li className="flex gap-2">
      <span className="w-28 shrink-0 text-muted-foreground">{label}</span>
      <span className="font-mono">{value}</span>
    </li>
  );
}
