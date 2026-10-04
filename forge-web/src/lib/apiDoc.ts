/**
 * A thin view over the server's OpenAPI document.
 *
 * The console renders its API reference from the same document a client
 * generator would use, so the docs cannot describe an endpoint the server
 * does not serve. The document is public, so no token is needed to read it.
 */

import { API_URL } from "@/lib/api";

export interface Operation {
  method: string;
  path: string;
  summary: string;
  operationId: string;
  tag: string;
  permission: string | null;
  description: string | null;
  isPublic: boolean;
}

export interface ApiDocument {
  version: string;
  title: string;
  paths: Operation[];
  tags: { name: string; description: string }[];
}

const METHOD_ORDER = ["get", "post", "put", "patch", "delete"];

/** Fetches and flattens the OpenAPI document into a list of operations. */
export async function fetchApiDoc(): Promise<ApiDocument> {
  const response = await fetch(`${API_URL}/openapi.json`);
  if (!response.ok) {
    throw new Error(`could not load the API document (${response.status})`);
  }
  const doc = await response.json();

  const operations: Operation[] = [];
  for (const [path, item] of Object.entries<Record<string, any>>(
    doc.paths ?? {},
  )) {
    for (const method of METHOD_ORDER) {
      const operation = item[method];
      if (!operation) continue;

      // A public operation declares `security: []`; an absent value inherits
      // the global scheme and so requires a token.
      const isPublic = Array.isArray(operation.security);
      const permission = operation.description?.match(/`([^`]+)` permission/)?.[1];

      operations.push({
        method,
        path,
        summary: operation.summary ?? "",
        operationId: operation.operationId ?? "",
        tag: (operation.tags ?? ["misc"])[0],
        permission: isPublic ? null : (permission ?? null),
        description: operation.description ?? null,
        isPublic,
      });
    }
  }

  operations.sort(
    (a, b) =>
      a.tag.localeCompare(b.tag) ||
      a.path.localeCompare(b.path) ||
      METHOD_ORDER.indexOf(a.method) - METHOD_ORDER.indexOf(b.method),
  );

  return {
    version: doc.info?.version ?? "",
    title: doc.info?.title ?? "Forge API",
    paths: operations,
    tags: describeTags(),
  };
}

/** What each group of endpoints is for, shown as the section heading. */
export function describeTags(): { name: string; description: string }[] {
  return [
    { name: "public", description: "Endpoints that need no token." },
    { name: "auth", description: "Sign in, register, and refresh tokens." },
    { name: "audit", description: "The immutable record of who did what." },
    { name: "users", description: "User accounts and their roles." },
    { name: "jobs", description: "Job definitions and their versions." },
    { name: "schedules", description: "Cron schedules and their next runs." },
    { name: "queues", description: "Queues, their capacity, and pausing." },
    { name: "workers", description: "Worker registration and the dispatch protocol." },
    { name: "executions", description: "Individual runs, their logs, and their outcomes." },
    { name: "workflows", description: "Multi-step DAG definitions and runs." },
    { name: "alerts", description: "Alert rules, alerts, and acknowledgements." },
    { name: "incidents", description: "Incidents grouping related alerts." },
    { name: "notifications", description: "Per-user notifications and preferences." },
    { name: "webhooks", description: "Outbound event subscriptions." },
    { name: "integrations", description: "External systems this tenant connects to." },
    { name: "savedViews", description: "Saved filter states for reuse." },
    { name: "undo", description: "Reverse a recent change." },
    { name: "dashboard", description: "Aggregates the console's overview reads." },
    { name: "search", description: "Cross-resource search." },
    { name: "assistant", description: "Questions and configuration proposals." },
    { name: "system", description: "Health, probes, and maintenance mode." },
    { name: "apiKeys", description: "Programmatic access keys." },
    { name: "admin", description: "Administrative operations." },
  ];
}

/** Builds a `curl` invocation for an operation. */
export function curlFor(
  operation: Operation,
  baseUrl: string,
  token: string | null,
  body?: unknown,
): string {
  const lines = [`curl -X ${operation.method.toUpperCase()} \\`];
  lines.push(`  '${baseUrl}${operation.path}' \\`);
  if (token) {
    lines.push(`  -H 'Authorization: Bearer ${token}' \\`);
  }
  lines.push(`  -H 'Content-Type: application/json'`);
  if (body) {
    lines.push(`  -d '${JSON.stringify(body)}'`);
  }
  return lines.join("\n");
}
