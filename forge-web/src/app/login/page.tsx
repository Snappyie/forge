"use client";

/**
 * Sign-in page (design 03-sign-in.png).
 *
 * Split layout: left half has the sign-in form, right half shows a dark panel
 * with production status. Includes personal access token sign-in and error
 * handling.
 */

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useState } from "react";
import { AlertTriangle, Key, Loader2 } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export default function LoginPage() {
  const router = useRouter();
  const { signIn } = useAuth();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function onSubmit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);

    try {
      await signIn(email.trim(), password);
      router.replace("/");
    } catch (cause) {
      setError(
        cause instanceof ApiError
          ? cause.message
          : "Could not reach the server",
      );
    } finally {
      setBusy(false);
    }
  }

  async function onTokenSignIn(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await api.post("/auth/token-login", { token: token.trim() });
      router.replace("/");
    } catch (cause) {
      setError(
        cause instanceof ApiError
          ? cause.message
          : "Invalid token or server unreachable",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="grid min-h-screen lg:grid-cols-[1fr_1fr]">
      {/* ─── Left: Sign-in form ─── */}
      <div className="flex items-center justify-center px-6 py-12">
        <div className="w-full max-w-[420px]">
          {/* Logo */}
          <div className="mb-8 flex items-center gap-2.5">
            <span className="grid size-9 place-items-center rounded-full bg-emerald-700 text-sm font-bold text-white">
              F
            </span>
            <span className="text-lg font-semibold tracking-tight">Forge</span>
          </div>

          <h1 className="text-2xl font-semibold tracking-tight">Sign in</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Access the console for your tenant.
          </p>

          {/* Error banner */}
          {error && (
            <div
              role="alert"
              className="mt-5 flex items-start gap-2.5 rounded-lg border border-red-200 bg-red-50 px-4 py-3 dark:border-red-900/50 dark:bg-red-950/30"
            >
              <AlertTriangle className="mt-0.5 size-4 shrink-0 text-red-600 dark:text-red-400" />
              <div>
                <p className="text-sm font-medium text-red-700 dark:text-red-400">
                  Email or password is incorrect.
                </p>
                <p className="mt-0.5 text-xs text-red-600/80 dark:text-red-400/70">
                  {error}
                </p>
              </div>
            </div>
          )}

          {/* Email/password form */}
          <form onSubmit={onSubmit} className="mt-6 flex flex-col gap-5">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="email">Work email</Label>
              <Input
                id="email"
                type="email"
                autoComplete="username"
                required
                autoFocus
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="neel@acme.com"
                className="h-11"
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <div className="flex items-center justify-between">
                <Label htmlFor="password">Password</Label>
                <button
                  type="button"
                  className="text-xs text-muted-foreground hover:text-foreground transition-colors"
                  tabIndex={-1}
                >
                  Forgot password?
                </button>
              </div>
              <Input
                id="password"
                type="password"
                autoComplete="current-password"
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="h-11"
              />
            </div>

            <Button type="submit" disabled={busy} className="h-11 w-full">
              {busy ? (
                <Loader2 className="mr-2 size-4 animate-spin" aria-hidden />
              ) : null}
              {busy ? "Signing in…" : "Sign in"}
            </Button>
          </form>

          {/* Divider */}
          <div className="my-6 flex items-center gap-3">
            <div className="h-px flex-1 bg-border" />
            <span className="text-xs text-muted-foreground">or</span>
            <div className="h-px flex-1 bg-border" />
          </div>

          {/* Token sign-in */}
          <form onSubmit={onTokenSignIn} className="flex flex-col gap-3">
            <Label htmlFor="pat">Personal access token</Label>
            <Input
              id="pat"
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="forge_••••••••••••••••••"
              className="h-11 font-mono text-sm"
            />
            <p className="text-xs text-muted-foreground">
              Create a token under Administration → API keys. Tokens are shown
              once.
            </p>
            <Button
              type="submit"
              variant="outline"
              disabled={busy || !token.trim()}
              className="h-11 w-full"
            >
              <Key className="mr-2 size-4" aria-hidden />
              Sign in with token
            </Button>
          </form>

          <p className="mt-6 text-center text-sm text-muted-foreground">
            New to Forge?{" "}
            <Link
              href="/register"
              className="font-medium text-foreground underline underline-offset-4"
            >
              Create a tenant
            </Link>
          </p>
        </div>
      </div>

      {/* ─── Right: Environment status panel ─── */}
      <div className="hidden bg-zinc-900 lg:flex lg:items-center lg:justify-center lg:px-12">
        <div className="w-full max-w-md">
          <p className="text-xs font-medium uppercase tracking-widest text-zinc-500">
            {process.env.NEXT_PUBLIC_FORGE_ENVIRONMENT || "Local"} status
          </p>

          <div className="mt-6 rounded-xl border border-zinc-800 bg-zinc-950 p-5">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium text-white">
                acme-{process.env.NEXT_PUBLIC_FORGE_ENVIRONMENT?.toLowerCase() || "local"}
              </span>
              <span className="rounded-full border border-zinc-700 px-2.5 py-0.5 text-xs font-mono text-zinc-400">
                eu-west-1
              </span>
            </div>

            <div className="mt-4 flex flex-col gap-3">
              <StatusRow label="Scheduler" value="tick 4s ago" color="green" />
              <StatusRow label="Workers" value="9 ready" color="green" />
              <StatusRow label="Queues" value="1 paused" color="amber" />
            </div>
          </div>

          <p className="mt-5 text-sm leading-relaxed text-zinc-500">
            {process.env.NEXT_PUBLIC_FORGE_ENVIRONMENT?.toLowerCase() === "production" ? (
              "You are signing in to a live production tenant. Destructive actions require a typed reason and are written to the audit log."
            ) : (
              `You are signing in to a ${process.env.NEXT_PUBLIC_FORGE_ENVIRONMENT || "Local"} tenant. Destructive actions are permitted for testing and development.`
            )}
          </p>
        </div>
      </div>
    </main>
  );
}

function StatusRow({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color: "green" | "amber" | "red";
}) {
  const dot =
    color === "green"
      ? "bg-emerald-500"
      : color === "amber"
        ? "bg-amber-500"
        : "bg-red-500";

  return (
    <div className="flex items-center justify-between">
      <div className="flex items-center gap-2">
        <span className={`size-2 rounded-full ${dot}`} />
        <span className="text-sm text-zinc-300">{label}</span>
      </div>
      <span className="font-mono text-xs text-zinc-500">{value}</span>
    </div>
  );
}
