"use client";

/**
 * The four states spec 7.2 requires on every page: loading, empty, error, and
 * permission-denied, plus an offline/degraded case.
 *
 * Centralising them keeps the states consistent and means a page cannot ship
 * without one.
 */

import { AlertTriangle, Inbox, Loader2, Lock, WifiOff } from "lucide-react";

import { ApiError } from "@/lib/api";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";

/** Shown while a request is in flight. */
export function LoadingState({ label = "Loading" }: { label?: string }) {
  return (
    <div
      role="status"
      aria-live="polite"
      className="flex flex-col items-center justify-center gap-2 py-12 text-muted-foreground"
    >
      <Loader2 className="size-5 animate-spin" aria-hidden />
      <p className="text-sm">{label}…</p>
    </div>
  );
}

/** Shown when a collection has no rows. */
export function EmptyState({
  title = "Nothing here yet",
  description,
  action,
}: {
  title?: string;
  description?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 py-12 text-center">
      <Inbox className="size-6 text-muted-foreground" aria-hidden />
      <p className="text-sm font-medium">{title}</p>
      {description ? (
        <p className="max-w-sm text-xs text-muted-foreground">{description}</p>
      ) : null}
      {action}
    </div>
  );
}

/**
 * Shown when a request fails.
 *
 * The server's message is shown rather than a generic string, because it is
 * already safe: the API returns a classified message and never a stack trace.
 */
export function ErrorState({
  error,
  onRetry,
}: {
  error: ApiError | Error | null;
  onRetry?: () => void;
}) {
  const offline = error instanceof ApiError && error.status === 0;

  const message = offline
    ? "Cannot reach the server"
    : error instanceof Error
      ? error.message
      : "Something went wrong";

  return (
    <div role="alert" className="flex flex-col items-center gap-3 py-12">
      <Alert variant={offline ? "default" : "destructive"} className="max-w-md">
        {offline ? (
          <WifiOff className="size-4" aria-hidden />
        ) : (
          <AlertTriangle className="size-4" aria-hidden />
        )}
        <AlertTitle>{message}</AlertTitle>
        {error instanceof ApiError && error.requestId ? (
          <AlertDescription>
            Request id: <code>{error.requestId}</code>
          </AlertDescription>
        ) : null}
      </Alert>
      {onRetry ? (
        <button
          type="button"
          onClick={onRetry}
          className="rounded-md border border-border px-3 py-1 text-xs font-medium hover:bg-muted"
        >
          Try again
        </button>
      ) : null}
    </div>
  );
}

/** Shown when the caller is authenticated but lacks permission. */
export function ForbiddenState({ message }: { message?: string }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 py-12 text-center">
      <Lock className="size-6 text-muted-foreground" aria-hidden />
      <p className="text-sm font-medium">You do not have permission to view this</p>
      <p className="max-w-sm text-xs text-muted-foreground">
        {message ?? "Ask an administrator for the access your role requires."}
      </p>
    </div>
  );
}

/**
 * Renders whichever state applies, so a page writes one call rather than four
 * conditionals.
 */
export function AsyncBoundary({
  state,
  error,
  forbidden,
  empty,
  onRetry,
  loadingLabel,
  emptyTitle,
  emptyDescription,
  emptyAction,
  children,
}: {
  state: "loading" | "ready" | "error";
  error: ApiError | null;
  forbidden: boolean;
  empty: boolean;
  onRetry?: () => void;
  loadingLabel?: string;
  emptyTitle?: string;
  emptyDescription?: string;
  /** Offered on the empty state, so it is a dead end with somewhere to go. */
  emptyAction?: React.ReactNode;
  children: React.ReactNode;
}) {
  if (state === "loading") return <LoadingState label={loadingLabel} />;
  if (state === "error") {
    return forbidden ? <ForbiddenState /> : <ErrorState error={error} onRetry={onRetry} />;
  }
  if (empty) {
    return (
      <EmptyState
        title={emptyTitle}
        description={emptyDescription}
        action={emptyAction}
      />
    );
  }
  return <>{children}</>;
}
