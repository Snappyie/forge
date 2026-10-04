"use client";

/**
 * Notification preferences (UI.md section 51).
 *
 * Persisted through the API, with the documented defaults returned when a user
 * has never saved anything.
 */

import { useEffect, useState } from "react";
import { Bell } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { useToast } from "@/lib/useToast";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";

const KINDS = [
  { value: "EXECUTION_FAILED", label: "Execution failed" },
  { value: "SLA_VIOLATION", label: "SLA violation" },
  { value: "QUEUE_BACKLOG", label: "Queue backlog" },
  { value: "WORKER_OFFLINE", label: "Worker offline" },
  { value: "LATENCY_SPIKE", label: "Latency spike" },
  { value: "SCHEDULE_MISSED", label: "Schedule missed" },
];

interface Preferences {
  enabled_kinds: string[];
  channels: { in_app?: boolean; email?: boolean; webhook?: boolean };
  quiet_hours_start: string | null;
  quiet_hours_end: string | null;
}

export function NotificationPreferences() {
  const stored = useQuery<Preferences>("/notification-preferences");
  const toast = useToast();
  const [kinds, setKinds] = useState<string[]>([]);
  const [channels, setChannels] = useState<Preferences["channels"]>({});
  const [busy, setBusy] = useState(false);

  // Seed the form once the stored preferences arrive.
  useEffect(() => {
    if (stored.data) {
      setKinds(stored.data.enabled_kinds ?? []);
      setChannels(stored.data.channels ?? {});
    }
  }, [stored.data]);

  async function save() {
    setBusy(true);
    try {
      await api.post("/notification-preferences/update", {
        enabled_kinds: kinds,
        channels,
      });
      toast.success("Notification preferences saved");
      stored.reload();
    } catch (error) {
      toast.error(
        "Could not save preferences",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-sm">
          <Bell className="size-3.5" aria-hidden />
          Notifications
        </CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <fieldset className="flex flex-col gap-1.5">
          <legend className="mb-1 text-xs text-muted-foreground">
            Alert kinds
          </legend>
          {KINDS.map((kind) => (
            <label
              key={kind.value}
              className="flex items-center gap-2 text-xs"
            >
              <Checkbox
                checked={kinds.includes(kind.value)}
                onCheckedChange={(checked) =>
                  setKinds((current) =>
                    checked
                      ? [...current, kind.value]
                      : current.filter((k) => k !== kind.value),
                  )
                }
                aria-label={kind.label}
              />
              {kind.label}
            </label>
          ))}
        </fieldset>

        <fieldset className="flex flex-col gap-1.5">
          <legend className="mb-1 text-xs text-muted-foreground">
            Channels
          </legend>
          {(["in_app", "email", "webhook"] as const).map((channel) => (
            <label key={channel} className="flex items-center gap-2 text-xs">
              <Checkbox
                checked={Boolean(channels[channel])}
                onCheckedChange={(checked) =>
                  setChannels((current) => ({ ...current, [channel]: Boolean(checked) }))
                }
                aria-label={channel}
              />
              {channel.replace("_", " ")}
            </label>
          ))}
        </fieldset>

        <Button
          size="sm"
          disabled={busy}
          onClick={save}
          className="self-start"
        >
          {busy ? "Saving…" : "Save preferences"}
        </Button>
      </CardContent>
    </Card>
  );
}
