"use client";

/**
 * Administration (spec 7.15).
 *
 * Users, API keys, and integrations. Every control is gated on the permission
 * it needs, so a developer sees this page without seeing controls they cannot
 * use.
 */

import { useState } from "react";
import { Copy, KeyRound, Plus, RefreshCw, ShieldOff, UserPlus } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { useQuery, useList } from "@/lib/useQuery";
import {
  formatTimestamp,
  roleCan,
  type ApiKey,
  type Role,
  type UserSummary,
} from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const ROLES: Role[] = ["OWNER", "ADMIN", "OPERATOR", "DEVELOPER", "AUDITOR", "VIEWER"];

export default function AdminPage() {
  const { session } = useAuth();
  const canReadUsers = roleCan(session?.role, "users:read");
  const canWriteUsers = roleCan(session?.role, "users:write");
  const canSettings = roleCan(session?.role, "settings:write");

  const users = useList<UserSummary>(canReadUsers ? "/users" : null);
  const keys = useList<ApiKey>("/api-keys");

  return (
    <div className="flex flex-col gap-8 p-6">
      <header className="flex items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Administration</h1>
          <p className="text-xs text-muted-foreground">
            Users, API keys, and integrations for this tenant.
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            users.reload();
            keys.reload();
          }}
          aria-label="Refresh"
        >
          <RefreshCw className="size-3.5" aria-hidden />
          Refresh
        </Button>
      </header>

      <section className="flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold">Users</h2>
          {canWriteUsers ? <CreateUserDialog onCreated={users.reload} /> : null}
        </div>

        <div className="rounded-lg border border-border">
          <AsyncBoundary
            state={users.state}
            error={users.error}
            forbidden={users.forbidden}
            empty={users.state === "ready" && users.rows.length === 0}
            onRetry={users.reload}
            loadingLabel="Loading users"
            emptyTitle="No users yet"
            emptyDescription="Create an account and assign it a role."
          >
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Email</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead>State</TableHead>
                  <TableHead>Created</TableHead>
                  {canWriteUsers ? (
                    <TableHead className="text-right">Actions</TableHead>
                  ) : null}
                </TableRow>
              </TableHeader>
              <TableBody>
                {users.rows.map((user) => (
                  <TableRow key={user.id}>
                    <TableCell>
                      <div className="font-medium">{user.email}</div>
                      {user.display_name ? (
                        <div className="text-xs text-muted-foreground">
                          {user.display_name}
                        </div>
                      ) : null}
                    </TableCell>
                    <TableCell>
                      <code className="text-xs">{user.role}</code>
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {user.disabled ? "disabled" : "active"}
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {formatTimestamp(user.created_at)}
                    </TableCell>
                    {canWriteUsers ? (
                      <TableCell className="text-right">
                        <DisableUserButton user={user} onDone={users.reload} />
                      </TableCell>
                    ) : null}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </AsyncBoundary>
        </div>
      </section>

      <section className="flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold">API keys</h2>
          {canWriteUsers ? <CreateApiKeyDialog onCreated={keys.reload} /> : null}
        </div>

        <div className="rounded-lg border border-border">
          <AsyncBoundary
            state={keys.state}
            error={keys.error}
            forbidden={keys.forbidden}
            empty={keys.state === "ready" && keys.rows.length === 0}
            onRetry={keys.reload}
            loadingLabel="Loading API keys"
            emptyTitle="No API keys"
            emptyDescription="A key is shown once at creation and cannot be recovered."
          >
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Prefix</TableHead>
                  <TableHead>State</TableHead>
                  <TableHead>Expires</TableHead>
                  {canWriteUsers ? (
                    <TableHead className="text-right">Actions</TableHead>
                  ) : null}
                </TableRow>
              </TableHeader>
              <TableBody>
                {keys.rows.map((key) => (
                  <TableRow key={key.id}>
                    <TableCell className="font-medium">{key.name}</TableCell>
                    <TableCell>
                      <code className="text-xs text-muted-foreground">
                        {key.prefix ?? "—"}
                      </code>
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {key.revoked ? "revoked" : "active"}
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {formatTimestamp(key.expires_at)}
                    </TableCell>
                    {canWriteUsers ? (
                      <TableCell className="text-right">
                        {!key.revoked ? (
                          <Button
                            variant="ghost"
                            size="sm"
                            aria-label={`Revoke ${key.name}`}
                            onClick={async () => {
                              if (
                                !window.confirm(`Revoke "${key.name}"? It stops working at once.`)
                              ) {
                                return;
                              }
                              try {
                                await api.post(`/api-keys/${key.id}/revoke`);
                                keys.reload();
                              } catch (cause) {
                                window.alert(
                                  cause instanceof ApiError ? cause.message : "the request failed",
                                );
                              }
                            }}
                          >
                            Revoke
                          </Button>
                        ) : null}
                      </TableCell>
                    ) : null}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </AsyncBoundary>
        </div>
      </section>

      {canSettings ? (
        <p className="text-xs text-muted-foreground">
          Integrations are configured per tenant and reference secrets by
          identifier, never by value.
        </p>
      ) : null}
    </div>
  );
}

function DisableUserButton({
  user,
  onDone,
}: {
  user: UserSummary;
  onDone: () => void;
}) {
  const [busy, setBusy] = useState(false);
  if (user.disabled) return null;

  return (
    <Button
      variant="ghost"
      size="sm"
      disabled={busy}
      onClick={async () => {
        if (!window.confirm(`Disable ${user.email}? They will be signed out.`)) return;
        setBusy(true);
        try {
          await api.post(`/users/${user.id}/disable`);
          onDone();
        } catch (cause) {
          window.alert(cause instanceof ApiError ? cause.message : "the request failed");
        } finally {
          setBusy(false);
        }
      }}
    >
      <ShieldOff className="size-3.5" aria-hidden />
      Disable
    </Button>
  );
}

function CreateUserDialog({ onCreated }: { onCreated: () => void }) {
  const [open, setOpen] = useState(false);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState<Role>("VIEWER");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await api.post("/users", { email: email.trim(), password, role });
      setOpen(false);
      setEmail("");
      setPassword("");
      onCreated();
    } catch (cause) {
      setError(
        cause instanceof ApiError
          ? (cause.fieldError("email") ?? cause.fieldError("password") ?? cause.message)
          : "could not reach the server",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <Button size="sm" onClick={() => setOpen((value) => !value)}>
        <UserPlus className="size-3.5" aria-hidden />
        New user
      </Button>

      {open ? (
        <form
          onSubmit={submit}
          className="mt-2 flex flex-wrap items-end gap-3 rounded-lg border border-border p-4"
        >
          <div className="flex min-w-[14rem] flex-col gap-1.5">
            <Label htmlFor="user-email">Email</Label>
            <Input
              id="user-email"
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              required
            />
          </div>
          <div className="flex min-w-[12rem] flex-col gap-1.5">
            <Label htmlFor="user-password">Password</Label>
            <Input
              id="user-password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
            />
            <p className="text-xs text-muted-foreground">At least 12 characters.</p>
          </div>
          <div className="flex w-36 flex-col gap-1.5">
            <Label htmlFor="user-role">Role</Label>
            <Select value={role} onValueChange={(value) => value && setRole(value as Role)}>
              <SelectTrigger id="user-role">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ROLES.map((r) => (
                  <SelectItem key={r} value={r}>
                    {r}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <Button type="submit" disabled={busy}>
            {busy ? "Creating…" : "Create"}
          </Button>
          {error ? (
            <p role="alert" className="w-full text-xs text-destructive">
              {error}
            </p>
          ) : null}
        </form>
      ) : null}
    </>
  );
}

function CreateApiKeyDialog({ onCreated }: { onCreated: () => void }) {
  const [name, setName] = useState("");
  const [issued, setIssued] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      const key = await api.post<{ key: string }>("/api-keys", { name: name.trim() });
      // Shown once; the server keeps only a hash, so it cannot be shown again.
      setIssued(key.key);
      setName("");
      onCreated();
    } catch (cause) {
      setError(
        cause instanceof ApiError ? cause.message : "could not reach the server",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col items-end gap-2">
      <form onSubmit={submit} className="flex items-end gap-2">
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Key name"
          aria-label="Key name"
          className="w-40"
          required
        />
        <Button type="submit" size="sm" disabled={busy}>
          <KeyRound className="size-3.5" aria-hidden />
          Issue
        </Button>
      </form>

      {issued ? (
        <div className="flex items-center gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2">
          <code className="font-mono text-xs">{issued}</code>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Copy key"
            onClick={() => navigator.clipboard?.writeText(issued)}
          >
            <Copy className="size-3.5" aria-hidden />
          </Button>
          <span className="text-[10px] text-amber-700 dark:text-amber-400">
            shown once — store it now
          </span>
        </div>
      ) : null}

      {error ? (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  );
}
