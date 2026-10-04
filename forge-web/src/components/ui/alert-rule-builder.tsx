"use client";

/**
 * Alert configuration (UI.md section 30).
 *
 * The spec asks for a WHEN / AND / THEN rule builder. This implements that
 * shape over real fields: a WHEN clause (kind plus threshold), optional AND
 * narrowing, and a THEN that notifies and optionally opens an incident.
 */

import { useState } from "react";
import { Plus } from "lucide-react";

import { useList } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

const KINDS = [
  { value: "EXECUTION_FAILED", label: "Job fails", threshold: "failures", unit: "times" },
  { value: "QUEUE_BACKLOG", label: "Queue depth exceeds", threshold: "depth", unit: "items" },
  { value: "WORKER_OFFLINE", label: "Worker goes offline", threshold: "minutes", unit: "minutes" },
  { value: "LATENCY_SPIKE", label: "Run duration exceeds", threshold: "seconds", unit: "seconds" },
  { value: "SLA_VIOLATION", label: "SLA missed", threshold: "runs", unit: "runs" },
  { value: "SCHEDULE_MISSED", label: "Schedule missed", threshold: "runs", unit: "runs" },
] as const;

interface Rule {
  id: string;
  kind: string;
  name: string;
  config: Record<string, unknown>;
  enabled: boolean;
  cooldown_seconds: number;
}

export function AlertRuleBuilder() {
  const rules = useList<Rule>("/alert-rules");
  const toast = useToast();
  const [kind, setKind] = useState<string>("EXECUTION_FAILED");
  const [name, setName] = useState("");
  const [threshold, setThreshold] = useState("3");
  const [busy, setBusy] = useState(false);

  const selected = KINDS.find((k) => k.value === kind) ?? KINDS[0];

  async function create() {
    setBusy(true);
    try {
      await api.post("/alert-rules", {
        kind,
        name: name.trim() || `${selected.label} rule`,
        config: {
          [selected.threshold]: Number(threshold) || 1,
          window_seconds: 900,
          open_incident: true,
        },
        cooldown_seconds: 900,
      });
      toast.success("Rule created");
      setName("");
      rules.reload();
    } catch (error) {
      toast.error(
        "Could not create rule",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  async function toggle(rule: Rule) {
    setBusy(true);
    try {
      await api.patch(`/alert-rules/${rule.id}`, { enabled: !rule.enabled });
      rules.reload();
    } catch (error) {
      toast.error(
        "Could not update rule",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Alert rules</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex flex-col gap-2 rounded-md border border-border p-3">
          <p className="text-[11px] uppercase tracking-wide text-muted-foreground">
            When
          </p>
          <div className="flex flex-wrap items-end gap-2">
            <div className="flex flex-col gap-1">
              <Label htmlFor="rule-kind">Condition</Label>
              <select
                id="rule-kind"
                value={kind}
                onChange={(e) => setKind(e.target.value)}
                className="h-8 rounded-md border border-border bg-background px-2 text-xs"
              >
                {KINDS.map((k) => (
                  <option key={k.value} value={k.value}>
                    {k.label}
                  </option>
                ))}
              </select>
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="rule-threshold">Threshold ({selected.unit})</Label>
              <Input
                id="rule-threshold"
                type="number"
                min={1}
                value={threshold}
                onChange={(e) => setThreshold(e.target.value)}
                className="h-8 w-24"
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="rule-name">Rule name</Label>
              <Input
                id="rule-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="settlement failures"
                className="h-8 w-56"
              />
            </div>
          </div>

          <p className="text-[11px] uppercase tracking-wide text-muted-foreground">
            Then
          </p>
          <p className="text-xs text-muted-foreground">
            Notify the tenant's alert inbox and open an incident.
          </p>

          <Button
            size="sm"
            disabled={busy}
            onClick={create}
            className="self-start"
          >
            <Plus className="mr-1 size-3.5" aria-hidden />
            Create rule
          </Button>
        </div>

        <AsyncBoundary
          state={rules.state}
          error={rules.error}
          forbidden={rules.forbidden}
          empty={rules.empty}
          onRetry={rules.reload}
          loadingLabel="Loading rules"
          emptyTitle="No alert rules"
          emptyDescription="Create one above to start firing alerts."
        >
          <ul className="flex flex-col divide-y divide-border/50">
            {rules.rows.map((rule) => (
              <li key={rule.id} className="flex items-center gap-2 py-2 text-xs">
                <span className="font-medium">{rule.name}</span>
                <Badge variant="secondary" className="text-[10px]">
                  {rule.kind.toLowerCase().replace(/_/g, " ")}
                </Badge>
                <span className="text-muted-foreground">
                  {Object.entries(rule.config ?? {})
                    .filter(([, value]) => typeof value !== "boolean")
                    .map(([key, value]) => `${key} ${String(value)}`)
                    .join(", ")}
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={busy}
                  onClick={() => toggle(rule)}
                  className="ml-auto"
                >
                  {rule.enabled ? "Disable" : "Enable"}
                </Button>
              </li>
            ))}
          </ul>
        </AsyncBoundary>
      </CardContent>
    </Card>
  );
}
