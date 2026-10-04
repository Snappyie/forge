"use client";

/**
 * Webhooks (UI.md section 75).
 *
 * Manage subscriptions, and show the recorded outcome of a test delivery.
 * A test reports what actually happened; nothing here claims delivery that the
 * outbox publisher has not confirmed.
 */

import { useState } from "react";
import { Plus, Send, Trash2, Webhook } from "lucide-react";

import { useList } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { formatRelative } from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useUnsavedChanges } from "@/lib/useUnsavedChanges";

interface Hook {
  id: string;
  name: string;
  url: string;
  auth_kind: string;
  has_secret: boolean;
  events: string[];
  max_retries: number;
  timeout_seconds: number;
  enabled: boolean;
  last_success_at: string | null;
  last_failure_at: string | null;
}

export default function WebhooksPage() {
  const webhooks = useList<Hook>("/webhooks");
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [authKind, setAuthKind] = useState("NONE");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The spec warns before losing a partially filled webhook.
  useUnsavedChanges(open && (name !== "" || url !== "" || secret !== ""));

  async function create() {
    setBusy(true);
    setError(null);
    try {
      await api.post("/webhooks", {
        name,
        url,
        auth_kind: authKind,
        ...(secret ? { secret } : {}),
      });
      toast.success("Webhook created");
      setOpen(false);
      setName("");
      setUrl("");
      setSecret("");
      setAuthKind("NONE");
      webhooks.reload();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Could not create webhook");
    } finally {
      setBusy(false);
    }
  }

  async function test(hook: Hook) {
    setBusy(true);
    try {
      const result = await api.post<{ detail: string }>(`/webhooks/${hook.id}/test`, {});
      toast.info("Test delivery queued", result.detail);
    } catch (caught) {
      toast.error(
        "Test failed",
        caught instanceof Error ? caught.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  async function remove(hook: Hook) {
    setBusy(true);
    try {
      await api.delete(`/webhooks/${hook.id}`);
      toast.success("Webhook deleted");
      webhooks.reload();
    } catch (caught) {
      toast.error(
        "Could not delete",
        caught instanceof Error ? caught.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex items-center justify-between gap-3">
        <div>
          <h1 className="flex items-center gap-2 text-lg font-semibold">
            <Webhook className="size-4" aria-hidden />
            Webhooks
          </h1>
          <p className="text-xs text-muted-foreground">
            Outbound event subscriptions. Secrets are stored hashed and never shown again.
          </p>
        </div>
        <Button size="sm" onClick={() => setOpen(true)}>
          <Plus className="mr-1 size-3.5" aria-hidden />
          New webhook
        </Button>
      </header>

      <AsyncBoundary
        state={webhooks.state}
        error={webhooks.error}
        forbidden={webhooks.forbidden}
        empty={webhooks.empty}
        onRetry={webhooks.reload}
        loadingLabel="Loading webhooks"
        emptyTitle="No webhooks configured"
        emptyDescription="Create a subscription to receive events on your own endpoint."
      >
        <div className="flex flex-col gap-3">
          {webhooks.rows.map((hook) => (
            <Card key={hook.id}>
              <CardHeader className="flex-row items-center justify-between">
                <CardTitle className="text-sm">{hook.name}</CardTitle>
                <div className="flex items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy}
                    onClick={() => test(hook)}
                  >
                    <Send className="mr-1 size-3.5" aria-hidden />
                    Test webhook
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy}
                    onClick={() => remove(hook)}
                    aria-label={`Delete ${hook.name}`}
                  >
                    <Trash2 className="size-3.5" aria-hidden />
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="flex flex-col gap-2 text-xs">
                <p className="font-mono break-all">{hook.url}</p>
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant="secondary" className="text-[10px]">
                    {hook.auth_kind}
                  </Badge>
                  {hook.has_secret ? (
                    <Badge variant="secondary" className="text-[10px]">
                      secret set
                    </Badge>
                  ) : null}
                  <Badge variant="secondary" className="text-[10px]">
                    {hook.max_retries} retries
                  </Badge>
                  <Badge variant="secondary" className="text-[10px]">
                    {hook.timeout_seconds}s timeout
                  </Badge>
                </div>
                <p className="text-muted-foreground">
                  Events:{" "}
                  {hook.events.length === 0 ? "all" : hook.events.join(", ")}
                </p>
                <p className="text-muted-foreground">
                  {hook.last_success_at
                    ? `Last success ${formatRelative(hook.last_success_at)}`
                    : "Never delivered"}
                  {hook.last_failure_at
                    ? ` · last failure ${formatRelative(hook.last_failure_at)}`
                    : ""}
                </p>
              </CardContent>
            </Card>
          ))}
        </div>
      </AsyncBoundary>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>New webhook</DialogTitle>
            <DialogDescription>
              The endpoint must be an absolute http or https URL. A secret, if
              given, is hashed before storage and cannot be retrieved afterwards.
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1">
              <Label htmlFor="hook-name">Name</Label>
              <Input
                id="hook-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="ops-alerts"
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="hook-url">URL</Label>
              <Input
                id="hook-url"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://example.com/forge"
              />
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor="hook-auth">Authentication</Label>
              <select
                id="hook-auth"
                value={authKind}
                onChange={(e) => setAuthKind(e.target.value)}
                className="h-8 rounded-md border border-border bg-background px-2 text-xs"
              >
                <option value="NONE">None</option>
                <option value="BEARER">Bearer</option>
                <option value="HMAC">HMAC</option>
                <option value="BASIC">Basic</option>
              </select>
            </div>
            {authKind !== "NONE" ? (
              <div className="flex flex-col gap-1">
                <Label htmlFor="hook-secret">Secret</Label>
                <Input
                  id="hook-secret"
                  type="password"
                  value={secret}
                  onChange={(e) => setSecret(e.target.value)}
                  autoComplete="new-password"
                />
              </div>
            ) : null}
            {error ? (
              <p role="alert" className="text-xs text-red-600">
                {error}
              </p>
            ) : null}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button
              onClick={create}
              disabled={busy || name.trim() === "" || url.trim() === ""}
            >
              {busy ? "Creating…" : "Create webhook"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
