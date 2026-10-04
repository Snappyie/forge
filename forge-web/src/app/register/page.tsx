"use client";

/**
 * Registration page (design 02-register.png).
 *
 * Split layout: left half has the registration form with tenant name, email,
 * and password with strength indicator. Right half shows a dark panel with
 * the "Three steps to your first execution" onboarding guide.
 */

import Link from "next/link";
import { useRouter } from "next/navigation";
import { useMemo, useState } from "react";
import { AlertCircle, Loader2 } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

function getPasswordStrength(pw: string): {
  label: string;
  percent: number;
  color: string;
} {
  let score = 0;
  if (pw.length >= 8) score++;
  if (pw.length >= 12) score++;
  if (/[A-Z]/.test(pw)) score++;
  if (/[0-9]/.test(pw)) score++;
  if (/[^A-Za-z0-9]/.test(pw)) score++;

  if (score <= 1) return { label: "Weak", percent: 20, color: "bg-red-500" };
  if (score <= 2) return { label: "Fair", percent: 40, color: "bg-orange-500" };
  if (score <= 3)
    return { label: "Good", percent: 60, color: "bg-yellow-500" };
  if (score <= 4)
    return { label: "Strong", percent: 80, color: "bg-emerald-500" };
  return { label: "Strong", percent: 100, color: "bg-emerald-500" };
}

export default function RegisterPage() {
  const router = useRouter();
  const { signIn } = useAuth();
  const [tenantName, setTenantName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const strength = useMemo(
    () => getPasswordStrength(password),
    [password],
  );

  async function onSubmit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);

    try {
      await api.post("/auth/register", {
        email: email.trim(),
        password,
        tenant_name: tenantName.trim() || undefined,
      });
      await signIn(email.trim(), password);
      router.replace("/");
    } catch (cause) {
      setError(
        cause instanceof ApiError
          ? cause.fieldError("password") ?? cause.message
          : "Could not reach the server",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="grid min-h-screen lg:grid-cols-[1fr_1fr]">
      {/* ─── Left: Registration form ─── */}
      <div className="flex items-center justify-center px-6 py-12">
        <div className="w-full max-w-[420px]">
          {/* Logo */}
          <div className="mb-8 flex items-center gap-2.5">
            <span className="grid size-9 place-items-center rounded-full bg-emerald-700 text-sm font-bold text-white">
              F
            </span>
            <span className="text-lg font-semibold tracking-tight">Forge</span>
          </div>

          <h1 className="text-2xl font-semibold tracking-tight">
            Create your tenant
          </h1>
          <p className="mt-1 text-sm text-muted-foreground">
            A tenant is an isolated workspace. Every job, worker, and execution
            belongs to exactly one.
          </p>

          <form onSubmit={onSubmit} className="mt-6 flex flex-col gap-5">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="tenant">Tenant name</Label>
              <Input
                id="tenant"
                value={tenantName}
                onChange={(e) => setTenantName(e.target.value)}
                placeholder="Acme Payments"
                className="h-11"
                autoFocus
              />
              <p className="text-xs text-muted-foreground">
                Your team or environment. You can add more tenants later.
              </p>
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="email">Work email</Label>
              <Input
                id="email"
                type="email"
                autoComplete="username"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                required
                placeholder="neel@acme.com"
                className="h-11"
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="password">Password</Label>
              <Input
                id="password"
                type="password"
                autoComplete="new-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
                minLength={12}
                className="h-11"
              />
              {/* Strength bar */}
              {password.length > 0 && (
                <div className="mt-1 flex items-center gap-2">
                  <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
                    <div
                      className={`h-full rounded-full transition-all duration-300 ${strength.color}`}
                      style={{ width: `${strength.percent}%` }}
                    />
                  </div>
                  <span
                    className={`text-xs font-medium ${
                      strength.color.includes("emerald")
                        ? "text-emerald-600 dark:text-emerald-400"
                        : strength.color.includes("red")
                          ? "text-red-600 dark:text-red-400"
                          : "text-amber-600 dark:text-amber-400"
                    }`}
                  >
                    {strength.label}
                  </span>
                </div>
              )}
              <p className="text-xs text-muted-foreground">
                At least 12 characters. Stored with argon2id, never recoverable.
              </p>
            </div>

            {error && (
              <div
                role="alert"
                className="flex items-start gap-2.5 rounded-lg border border-red-200 bg-red-50 px-4 py-3 dark:border-red-900/50 dark:bg-red-950/30"
              >
                <AlertCircle className="mt-0.5 size-4 shrink-0 text-red-600 dark:text-red-400" />
                <span className="text-sm text-red-700 dark:text-red-400">
                  {error}
                </span>
              </div>
            )}

            <Button type="submit" disabled={busy} className="h-11 w-full">
              {busy ? (
                <Loader2 className="mr-2 size-4 animate-spin" aria-hidden />
              ) : null}
              {busy ? "Creating…" : "Create tenant and continue"}
            </Button>
          </form>

          <p className="mt-6 text-center text-sm text-muted-foreground">
            Already have an account?{" "}
            <Link
              href="/login"
              className="font-medium text-foreground underline underline-offset-4"
            >
              Sign in
            </Link>
          </p>
        </div>
      </div>

      {/* ─── Right: Onboarding guide ─── */}
      <div className="hidden bg-zinc-900 lg:flex lg:items-center lg:justify-center lg:px-12">
        <div className="w-full max-w-md">
          <p className="text-xs font-medium uppercase tracking-widest text-zinc-500">
            What happens next
          </p>
          <h2 className="mt-3 text-2xl font-semibold tracking-tight text-white">
            Three steps to your first execution
          </h2>

          <div className="mt-8 flex flex-col gap-6">
            <OnboardingStep
              number={1}
              title="Name a job"
              description="A reusable definition of work: what to run, how often, and what happens when it fails."
            />
            <OnboardingStep
              number={2}
              title="Publish a version"
              description="Drafts never run. Publishing freezes a version so a change can never alter work already dispatched."
            />
            <OnboardingStep
              number={3}
              title="Run it, or let it run"
              description="Trigger once by hand, or attach a schedule and a worker and let Forge do it for you."
            />
          </div>

          <p className="mt-8 text-sm text-zinc-600">
            No card, no cluster. Runs on one Postgres container to start.
          </p>
        </div>
      </div>
    </main>
  );
}

function OnboardingStep({
  number,
  title,
  description,
}: {
  number: number;
  title: string;
  description: string;
}) {
  return (
    <div className="flex gap-4">
      <span className="grid size-8 shrink-0 place-items-center rounded-full bg-emerald-700 text-sm font-bold text-white">
        {number}
      </span>
      <div>
        <h3 className="text-sm font-semibold text-white">{title}</h3>
        <p className="mt-0.5 text-sm leading-relaxed text-zinc-400">
          {description}
        </p>
      </div>
    </div>
  );
}
