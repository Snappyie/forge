"use client";

/**
 * Administration (spec 7.15).
 *
 * Users, API keys, and integrations. Every control is gated on the permission
 * it needs, so a developer sees this page without seeing controls they cannot
 * use.
 */

import { useState } from "react";
import { Bot, Copy, Globe, KeyRound, Plus, RefreshCw, Shield, ShieldOff, Trash2, UserPlus } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { useQuery, useList } from "@/lib/useQuery";
import {
  formatTimestamp,
  roleCan,
  type ApiKey,
  type IdentityProvider,
  type Role,
  type ServiceAccount,
  type UserSummary,
} from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { Badge } from "@/components/ui/badge";
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
  const serviceAccounts = useList<ServiceAccount>("/service-accounts");
  const oidcProviders = useList<IdentityProvider>(canSettings ? "/auth/oidc/providers" : null);

  return (
    <div className="flex flex-col gap-8 p-6">
      <header className="flex items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Administration</h1>
          <p className="text-xs text-muted-foreground">
            Users, API keys, service accounts, and enterprise SSO for this tenant.
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            users.reload();
            keys.reload();
            serviceAccounts.reload();
            oidcProviders.reload();
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

      <section className="flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-sm font-semibold">Service Accounts</h2>
            <p className="text-xs text-muted-foreground">
              Scoped automation tokens for CI/CD pipelines and external integrations.
            </p>
          </div>
          {canWriteUsers ? <CreateServiceAccountDialog onCreated={serviceAccounts.reload} /> : null}
        </div>

        <div className="rounded-lg border border-border">
          <AsyncBoundary
            state={serviceAccounts.state}
            error={serviceAccounts.error}
            forbidden={serviceAccounts.forbidden}
            empty={serviceAccounts.state === "ready" && serviceAccounts.rows.length === 0}
            onRetry={serviceAccounts.reload}
            loadingLabel="Loading service accounts"
            emptyTitle="No service accounts yet"
            emptyDescription="Create a service account with scoped permissions."
          >
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Prefix</TableHead>
                  <TableHead>Scopes</TableHead>
                  <TableHead>Created</TableHead>
                  <TableHead>Status</TableHead>
                  {canWriteUsers ? <TableHead className="w-24 text-right">Actions</TableHead> : null}
                </TableRow>
              </TableHeader>
              <TableBody>
                {serviceAccounts.rows.map((sa) => (
                  <TableRow key={sa.id}>
                    <TableCell>
                      <div className="flex flex-col">
                        <span className="font-medium text-sm">{sa.name}</span>
                        {sa.description ? (
                          <span className="text-xs text-muted-foreground">{sa.description}</span>
                        ) : null}
                      </div>
                    </TableCell>
                    <TableCell>
                      <code className="font-mono text-xs">{sa.token_prefix}••••••••</code>
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-wrap gap-1">
                        {sa.scopes && sa.scopes.length > 0 ? (
                          sa.scopes.map((scope) => (
                            <Badge key={scope} variant="outline" className="font-mono text-[10px]">
                              {scope}
                            </Badge>
                          ))
                        ) : (
                          <span className="text-xs text-muted-foreground">None</span>
                        )}
                      </div>
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {formatTimestamp(sa.created_at)}
                    </TableCell>
                    <TableCell>
                      {sa.revoked_at ? (
                        <span className="inline-flex items-center rounded-md bg-destructive/10 px-2 py-0.5 text-xs font-medium text-destructive">
                          Revoked
                        </span>
                      ) : (
                        <span className="inline-flex items-center rounded-md bg-emerald-500/10 px-2 py-0.5 text-xs font-medium text-emerald-600 dark:text-emerald-400">
                          Active
                        </span>
                      )}
                    </TableCell>
                    {canWriteUsers ? (
                      <TableCell className="text-right">
                        {!sa.revoked_at ? (
                          <Button
                            variant="ghost"
                            size="sm"
                            className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                            onClick={async () => {
                              if (!window.confirm(`Revoke service account "${sa.name}"?`)) return;
                              try {
                                await api.post(`/service-accounts/${sa.id}/revoke`);
                                serviceAccounts.reload();
                              } catch (cause) {
                                window.alert(cause instanceof ApiError ? cause.message : "Revoke failed");
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
        <section className="flex flex-col gap-3">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-sm font-semibold">SSO / Identity Providers (OIDC)</h2>
              <p className="text-xs text-muted-foreground">
                Federated OpenID Connect providers for enterprise single sign-on.
              </p>
            </div>
            <RegisterOidcProviderDialog onCreated={oidcProviders.reload} />
          </div>

          <div className="rounded-lg border border-border">
            <AsyncBoundary
              state={oidcProviders.state}
              error={oidcProviders.error}
              forbidden={oidcProviders.forbidden}
              empty={oidcProviders.state === "ready" && oidcProviders.rows.length === 0}
              onRetry={oidcProviders.reload}
              loadingLabel="Loading identity providers"
              emptyTitle="No identity providers configured"
              emptyDescription="Add an OIDC provider (Okta, Google, Azure AD, Keycloak) to enable SSO."
            >
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Provider Name</TableHead>
                    <TableHead>Issuer URL</TableHead>
                    <TableHead>Client ID</TableHead>
                    <TableHead>Allowed Domains</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead className="w-24 text-right">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {oidcProviders.rows.map((prov) => (
                    <TableRow key={prov.id}>
                      <TableCell className="font-medium">{prov.name}</TableCell>
                      <TableCell className="font-mono text-xs max-w-[200px] truncate">{prov.issuer}</TableCell>
                      <TableCell className="font-mono text-xs">{prov.client_id}</TableCell>
                      <TableCell className="text-xs text-muted-foreground">
                        {prov.allowed_email_domains && prov.allowed_email_domains.length > 0
                          ? prov.allowed_email_domains.join(", ")
                          : "Any"}
                      </TableCell>
                      <TableCell>
                        <span className="inline-flex items-center rounded-md bg-emerald-500/10 px-2 py-0.5 text-xs font-medium text-emerald-600 dark:text-emerald-400">
                          Enabled
                        </span>
                      </TableCell>
                      <TableCell className="text-right">
                        <Button
                          variant="ghost"
                          size="sm"
                          className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                          onClick={async () => {
                            if (!window.confirm(`Delete identity provider "${prov.name}"?`)) return;
                            try {
                              await api.delete(`/auth/oidc/providers/${prov.id}`);
                              oidcProviders.reload();
                            } catch (cause) {
                              window.alert(cause instanceof ApiError ? cause.message : "Delete failed");
                            }
                          }}
                        >
                          <Trash2 className="size-3.5" aria-hidden />
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </AsyncBoundary>
          </div>
        </section>
      ) : null}

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

const SERVICE_ACCOUNT_SCOPES = [
  "jobs:read",
  "jobs:write",
  "executions:read",
  "executions:write",
  "schedules:read",
  "schedules:write",
  "workflows:read",
  "workflows:write",
  "workers:read",
  "system:read",
];

function CreateServiceAccountDialog({ onCreated }: { onCreated: () => void }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [selectedScopes, setSelectedScopes] = useState<string[]>([
    "jobs:read",
    "executions:read",
  ]);
  const [issued, setIssued] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  function toggleScope(scope: string) {
    setSelectedScopes((prev) =>
      prev.includes(scope) ? prev.filter((s) => s !== scope) : [...prev, scope],
    );
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      const res = await api.post<{ token: string }>("/service-accounts", {
        name: name.trim(),
        description: description.trim() || undefined,
        scopes: selectedScopes,
      });
      setIssued(res.token);
      setName("");
      setDescription("");
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
    <>
      <Button size="sm" onClick={() => setOpen((value) => !value)}>
        <Bot className="size-3.5" aria-hidden />
        New service account
      </Button>

      {open ? (
        <form
          onSubmit={submit}
          className="mt-2 flex flex-col gap-4 rounded-lg border border-border p-4 w-full"
        >
          <div className="flex flex-wrap gap-4">
            <div className="flex min-w-[14rem] flex-col gap-1.5 flex-1">
              <Label htmlFor="sa-name">Account Name</Label>
              <Input
                id="sa-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. ci-cd-deployer"
                required
              />
            </div>
            <div className="flex min-w-[14rem] flex-col gap-1.5 flex-1">
              <Label htmlFor="sa-desc">Description</Label>
              <Input
                id="sa-desc"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="e.g. GitHub Actions pipeline deployment token"
              />
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <Label>Scopes</Label>
            <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-5 gap-2">
              {SERVICE_ACCOUNT_SCOPES.map((scope) => {
                const checked = selectedScopes.includes(scope);
                return (
                  <label
                    key={scope}
                    className={`flex items-center gap-2 rounded-md border p-2 text-xs font-mono cursor-pointer transition-colors ${
                      checked
                        ? "border-primary bg-primary/5 text-primary font-medium"
                        : "border-border text-muted-foreground hover:bg-muted/50"
                    }`}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggleScope(scope)}
                      className="size-3.5 rounded border-input"
                    />
                    <span>{scope}</span>
                  </label>
                );
              })}
            </div>
          </div>

          <div className="flex items-center justify-between pt-2">
            <Button type="submit" disabled={busy || !name.trim()}>
              {busy ? "Issuing token…" : "Issue Token"}
            </Button>
            <Button variant="ghost" type="button" onClick={() => setOpen(false)}>
              Cancel
            </Button>
          </div>

          {issued ? (
            <div className="flex items-center justify-between gap-2 rounded-md border border-emerald-500/40 bg-emerald-500/10 px-4 py-3">
              <div className="flex flex-col gap-1">
                <span className="text-xs font-semibold text-emerald-800 dark:text-emerald-300">
                  Service Account Token (shown once — store it now)
                </span>
                <code className="font-mono text-xs break-all text-emerald-950 dark:text-emerald-100">{issued}</code>
              </div>
              <Button
                variant="ghost"
                size="icon"
                type="button"
                aria-label="Copy token"
                onClick={() => navigator.clipboard?.writeText(issued)}
              >
                <Copy className="size-4" aria-hidden />
              </Button>
            </div>
          ) : null}

          {error ? (
            <p role="alert" className="text-xs text-destructive">
              {error}
            </p>
          ) : null}
        </form>
      ) : null}
    </>
  );
}

function RegisterOidcProviderDialog({ onCreated }: { onCreated: () => void }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [issuer, setIssuer] = useState("");
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [scopes, setScopes] = useState("openid, profile, email");
  const [domains, setDomains] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      const scopeArray = scopes.split(",").map((s) => s.trim()).filter(Boolean);
      const domainArray = domains.split(",").map((d) => d.trim()).filter(Boolean);
      await api.post("/auth/oidc/providers", {
        name: name.trim().toLowerCase(),
        issuer: issuer.trim(),
        client_id: clientId.trim(),
        client_secret: clientSecret.trim(),
        scopes: scopeArray.length > 0 ? scopeArray : undefined,
        allowed_email_domains: domainArray.length > 0 ? domainArray : undefined,
      });
      setOpen(false);
      setName("");
      setIssuer("");
      setClientId("");
      setClientSecret("");
      onCreated();
    } catch (cause) {
      setError(
        cause instanceof ApiError ? cause.message : "could not register provider",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <Button size="sm" onClick={() => setOpen((value) => !value)}>
        <Globe className="size-3.5" aria-hidden />
        Add Provider
      </Button>

      {open ? (
        <form
          onSubmit={submit}
          className="mt-2 flex flex-col gap-3 rounded-lg border border-border p-4 w-full"
        >
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="oidc-name">Provider Name (Slug)</Label>
              <Input
                id="oidc-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. okta, google, azure"
                required
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="oidc-issuer">Issuer URL</Label>
              <Input
                id="oidc-issuer"
                value={issuer}
                onChange={(e) => setIssuer(e.target.value)}
                placeholder="https://accounts.google.com"
                required
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="oidc-client-id">Client ID</Label>
              <Input
                id="oidc-client-id"
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
                required
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="oidc-secret">Client Secret</Label>
              <Input
                id="oidc-secret"
                type="password"
                value={clientSecret}
                onChange={(e) => setClientSecret(e.target.value)}
                required
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="oidc-scopes">Scopes (comma-separated)</Label>
              <Input
                id="oidc-scopes"
                value={scopes}
                onChange={(e) => setScopes(e.target.value)}
                placeholder="openid, profile, email"
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="oidc-domains">Allowed Email Domains (optional)</Label>
              <Input
                id="oidc-domains"
                value={domains}
                onChange={(e) => setDomains(e.target.value)}
                placeholder="company.com, acme.corp"
              />
            </div>
          </div>

          <div className="flex items-center justify-between pt-2">
            <Button type="submit" disabled={busy || !name.trim() || !issuer.trim()}>
              {busy ? "Registering…" : "Register Provider"}
            </Button>
            <Button variant="ghost" type="button" onClick={() => setOpen(false)}>
              Cancel
            </Button>
          </div>

          {error ? (
            <p role="alert" className="text-xs text-destructive">
              {error}
            </p>
          ) : null}
        </form>
      ) : null}
    </>
  );
}
