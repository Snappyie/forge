"use client";

/**
 * Integrations (UI.md section 74, API keys in section 76).
 *
 * Reads the real API key inventory. Secrets are write-only, so the table shows
 * metadata and never a key value — the API does not return one.
 */

import { useState } from "react";
import { Copy, KeyRound, Plus, RefreshCw } from "lucide-react";

import { useList } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { IntegrationList } from "@/components/ui/integration-list";
import { useToast } from "@/lib/useToast";
import { formatTimestamp } from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

interface ApiKey {
  id: string;
  name: string;
  prefix: string;
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
  expires_at?: string | null;
}

export default function IntegrationsPage() {
  const keys = useList<ApiKey>("/api-keys");
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [issued, setIssued] = useState<{ prefix: string; key: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const [error, setError] = useState<string | null>(null);

  // Rotation replaces the secret and invalidates the old one immediately.
  async function rotate(key: ApiKey) {
    setBusy(true);
    setError(null);
    try {
      const rotated = await api.post<{ key: string; prefix: string }>(
        `/api-keys/${key.id}/rotate`,
        {},
      );
      setIssued({ prefix: rotated.prefix, key: rotated.key });
      toast.success("Key rotated", {
        label: "Back to keys",
        onClick: () => window.location.assign("/integrations"),
      });
      keys.reload();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Could not rotate key");
    } finally {
      setBusy(false);
    }
  }

  async function revoke(key: ApiKey) {
    setBusy(true);
    try {
      await api.post(`/api-keys/${key.id}/revoke`, {});
      toast.success("Key revoked");
      keys.reload();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Could not revoke key");
    } finally {
      setBusy(false);
    }
  }

  async function create() {
    setSaving(true);
    setError(null);
    try {
      const created = await api.post<{ prefix: string; key: string }>("/api-keys", {
        name,
      });
      setIssued(created);
      setCreating(false);
      setName("");
      keys.reload();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Could not create key");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Integrations</h1>
          <p className="text-xs text-muted-foreground">
            API keys for scripted access. Key values are shown once, at creation.
          </p>
        </div>
        <Button size="sm" onClick={() => setCreating(true)}>
          <Plus className="mr-1 size-3.5" aria-hidden />
          New API key
        </Button>
      </header>

      <IntegrationList />

      {issued ? (
        <div className="rounded-lg border border-emerald-500/40 bg-emerald-500/10 p-3">
          <p className="text-xs font-medium text-emerald-700 dark:text-emerald-400">
            Key created. Copy it now — it is not shown again.
          </p>
          <div className="mt-2 flex items-center gap-2">
            <code className="min-w-0 flex-1 truncate rounded bg-background px-2 py-1 font-mono text-xs">
              {issued.key}
            </code>
            <Button
              variant="outline"
              size="sm"
              onClick={() => navigator.clipboard?.writeText(issued.key)}
              aria-label="Copy API key"
            >
              <Copy className="size-3.5" aria-hidden />
            </Button>
          </div>
        </div>
      ) : null}

      <AsyncBoundary
        state={keys.state}
        error={keys.error}
        forbidden={keys.forbidden}
        empty={keys.empty}
        onRetry={keys.reload}
        loadingLabel="Loading API keys"
        emptyTitle="No API keys yet"
        emptyDescription="Create a key to let scripts and CI call the Forge API."
      >
        <div className="overflow-hidden rounded-lg border border-border">
          <table className="w-full text-sm">
            <caption className="sr-only">API keys for this tenant</caption>
            <thead className="bg-muted/40 text-left text-xs text-muted-foreground">
              <tr>
                <th scope="col" className="px-3 py-2 font-medium">Name</th>
                <th scope="col" className="px-3 py-2 font-medium">Prefix</th>
                <th scope="col" className="px-3 py-2 font-medium">Created</th>
                <th scope="col" className="px-3 py-2 font-medium">Last used</th>
                <th scope="col" className="px-3 py-2 font-medium">Status</th>
              </tr>
            </thead>
            <tbody>
              {keys.rows.map((key) => (
                <tr key={key.id} className="border-t border-border">
                  <td className="flex items-center gap-2 px-3 py-2">
                    <KeyRound className="size-3.5 text-muted-foreground" aria-hidden />
                    {key.name}
                  </td>
                  <td className="px-3 py-2 font-mono text-xs">{key.prefix}…</td>
                  <td className="px-3 py-2 text-xs text-muted-foreground">
                    {formatTimestamp(key.created_at)}
                  </td>
                  <td className="px-3 py-2 text-xs text-muted-foreground">
                    {key.last_used_at ? formatTimestamp(key.last_used_at) : "never"}
                  </td>
                  <td className="px-3 py-2">
                    <Badge variant="secondary" className="text-[10px]">
                      {key.revoked_at
                        ? "revoked"
                        : key.expires_at
                          ? `expires ${formatTimestamp(key.expires_at)}`
                          : "active"}
                    </Badge>
                    {!key.revoked_at ? (
                      <span className="flex items-center gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={busy}
                          onClick={() => rotate(key)}
                          aria-label={`Rotate ${key.name}`}
                        >
                          <RefreshCw className="size-3" aria-hidden />
                          Rotate
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          disabled={busy}
                          onClick={() => revoke(key)}
                          aria-label={`Revoke ${key.name}`}
                        >
                          Revoke
                        </Button>
                      </span>
                    ) : null}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </AsyncBoundary>

      <Dialog open={creating} onOpenChange={setCreating}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>New API key</DialogTitle>
            <DialogDescription>
              The key is shown once after creation and cannot be retrieved again.
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-2">
            <Label htmlFor="key-name">Name</Label>
            <Input
              id="key-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="ci-deploy"
            />
            {error ? (
              <p role="alert" className="text-xs text-red-600">
                {error}
              </p>
            ) : null}
          </div>

          <DialogFooter>
            <Button variant="outline" onClick={() => setCreating(false)}>
              Cancel
            </Button>
            <Button onClick={create} disabled={saving || name.trim() === ""}>
              {saving ? "Creating…" : "Create key"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
