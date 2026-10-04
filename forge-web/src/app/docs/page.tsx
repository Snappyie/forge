"use client";

/**
 * Developer documentation.
 *
 * The endpoint reference is rendered from the server's own OpenAPI document,
 * so what is documented here is exactly what the server serves. The hand-
 * written sections cover integration concerns a schema cannot express: how to
 * authenticate, how retries work, and which mistakes are easy to make.
 */

import { useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { Check, Copy, ExternalLink, Search } from "lucide-react";

import { API_URL } from "@/lib/api";
import { curlFor, fetchApiDoc, type ApiDocument, type Operation } from "@/lib/apiDoc";
import { AsyncBoundary } from "@/components/states";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "cn";

const METHOD_TONE: Record<string, string> = {
  get: "border-sky-500/40 text-sky-600 dark:text-sky-400",
  post: "border-emerald-500/40 text-emerald-600 dark:text-emerald-400",
  put: "border-amber-500/40 text-amber-700 dark:text-amber-400",
  patch: "border-amber-500/40 text-amber-700 dark:text-amber-400",
  delete: "border-red-500/40 text-red-600 dark:text-red-400",
};

export default function DocsPage() {
  const [doc, setDoc] = useState<ApiDocument | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    fetchApiDoc()
      .then((loaded) => {
        setDoc(loaded);
        setError(null);
      })
      .catch((cause: Error) => setError(cause.message))
      .finally(() => setLoading(false));
  }, []);

  const groups = useMemo(() => {
    if (!doc) return [];
    const needle = filter.trim().toLowerCase();
    const matching = needle
      ? doc.paths.filter(
          (op) =>
            op.path.toLowerCase().includes(needle) ||
            op.summary.toLowerCase().includes(needle) ||
            op.tag.toLowerCase().includes(needle),
        )
      : doc.paths;

    const byTag = new Map<string, Operation[]>();
    for (const op of matching) {
      byTag.set(op.tag, [...(byTag.get(op.tag) ?? []), op]);
    }
    return doc.tags
      .filter((tag) => byTag.has(tag.name))
      .map((tag) => ({ ...tag, operations: byTag.get(tag.name)! }));
  }, [doc, filter]);

  return (
    <div className="flex flex-col gap-6 p-6">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Developer documentation</h1>
          <p className="text-xs text-muted-foreground">
            Everything needed to integrate Forge. The reference below is
            generated from the same OpenAPI document a client generator reads.
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          render={
            <a href={`${API_URL}/openapi.json`} target="_blank" rel="noreferrer" />
          }
        >
          <ExternalLink className="mr-1 size-3.5" aria-hidden />
          openapi.json
        </Button>
      </header>

      <Tabs defaultValue="start">
        <TabsList>
          <TabsTrigger value="start">Getting started</TabsTrigger>
          <TabsTrigger value="concepts">Concepts</TabsTrigger>
          <TabsTrigger value="recipes">Recipes</TabsTrigger>
          <TabsTrigger value="sdks">Worker SDKs</TabsTrigger>
          <TabsTrigger value="reference">API reference</TabsTrigger>
        </TabsList>

        <TabsContent value="start" className="mt-4 flex flex-col gap-4">
          <GettingStarted baseUrl={API_URL} />
        </TabsContent>

        <TabsContent value="concepts" className="mt-4">
          <Concepts />
        </TabsContent>

        <TabsContent value="recipes" className="mt-4">
          <Recipes baseUrl={API_URL} />
        </TabsContent>

        <TabsContent value="sdks" className="mt-4">
          <WorkerSdks />
        </TabsContent>

        <TabsContent value="reference" className="mt-4 flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <Search className="size-3.5 text-muted-foreground" aria-hidden />
            <Input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filter endpoints by path, tag, or summary"
              aria-label="Filter endpoints"
              className="max-w-sm"
            />
            {doc ? (
              <span className="text-xs text-muted-foreground">
                {groups.reduce((sum, g) => sum + g.operations.length, 0)} of{" "}
                {doc.paths.length} endpoints
              </span>
            ) : null}
          </div>

          <AsyncBoundary
            state={loading ? "loading" : error ? "error" : "ready"}
            error={error ? Object.assign(new Error(error), {}) as never : null}
            forbidden={false}
            empty={false}
            loadingLabel="Loading the API document"
          >
            {doc ? (
              groups.length === 0 ? (
                <p className="py-8 text-center text-xs text-muted-foreground">
                  No endpoint matches “{filter}”.
                </p>
              ) : (
                <div className="flex flex-col gap-6">
                  {groups.map((group) => (
                    <section key={group.name}>
                      <h2 className="mb-1 text-sm font-semibold">
                        {group.name}
                      </h2>
                      <p className="mb-2 text-xs text-muted-foreground">
                        {group.description}
                      </p>
                      <div className="flex flex-col gap-2">
                        {group.operations.map((operation) => (
                          <OperationRow
                            key={`${operation.method}-${operation.path}`}
                            operation={operation}
                            baseUrl={API_URL}
                          />
                        ))}
                      </div>
                    </section>
                  ))}
                </div>
              )
            ) : null}
          </AsyncBoundary>
        </TabsContent>
      </Tabs>
    </div>
  );
}

function OperationRow({
  operation,
  baseUrl,
}: {
  operation: Operation;
  baseUrl: string;
}) {
  return (
    <Card>
      <CardHeader className="flex-row flex-wrap items-center gap-2 pb-2">
        <Badge
          variant="outline"
          className={cn("font-mono text-[10px] uppercase", METHOD_TONE[operation.method])}
        >
          {operation.method}
        </Badge>
        <code className="font-mono text-xs">{operation.path}</code>
        {operation.isPublic ? (
          <Badge variant="secondary" className="text-[10px]">
            public
          </Badge>
        ) : operation.permission ? (
          <Badge variant="outline" className="text-[10px]">
            {operation.permission}
          </Badge>
        ) : null}
        <span className="ml-auto text-xs text-muted-foreground">
          {operation.summary}
        </span>
      </CardHeader>
      <CardContent className="pt-0">
        <CopyBlock
          text={curlFor(operation, baseUrl, operation.isPublic ? null : "<token>")}
        />
      </CardContent>
    </Card>
  );
}

function GettingStarted({ baseUrl }: { baseUrl: string }) {
  return (
    <div className="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">1. Get a token</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 text-xs">
          <p className="text-muted-foreground">
            Registration is closed by default. The first tenant is claimed once
            by its owner; everyone afterwards joins with an invitation token
            supplied as <code className="font-mono">invite_token</code>.
          </p>
          <CopyBlock
            text={`curl -X POST '${baseUrl}/auth/register' \\
  -H 'Content-Type: application/json' \\
  -d '{
    "email": "you@example.com",
    "password": "correct-horse-battery",
    "invite_token": "<uuid from your invite>"
  }'`}
          />
          <p className="text-muted-foreground">
            Sign in the same way without <code className="font-mono">invite_token</code>.
            The response carries an access token (short-lived) and a refresh
            token (rotating). Send the access token on every later call:
          </p>
          <CopyBlock text={`Authorization: Bearer <access_token>`} />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">2. Define a job, then a version</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 text-xs">
          <p className="text-muted-foreground">
            A job is a draft until one of its versions is published. An
            execution can only run against a published version, which is what
            makes a definition reviewable before anything runs.
          </p>
          <CopyBlock
            text={`# Create the job
JOB_ID=$(curl -s -X POST '${baseUrl}/jobs' \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \\
  -d '{"name":"Nightly Settlement","key":"nightly-settlement","priority":"CRITICAL"}' \\
  | python3 -c 'import sys,json; print(json.load(sys.stdin)["data"]["id"])')

# Add a version describing what it runs
VERSION_ID=$(curl -s -X POST "$BASE/jobs/$JOB_ID/versions" \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \\
  -d '{"config":{"url":"https://example.com/settle","timeout_seconds":300},
       "concurrency_policy":{"max_concurrent_executions":{"Bounded":2},"scope":"JOB","queue_id":null}}' \\
  | python3 -c 'import sys,json; print(json.load(sys.stdin)["data"]["id"])')

# Publish it
curl -X POST "$BASE/jobs/$JOB_ID/versions/$VERSION_ID/publish" \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{}'`}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">3. Schedule it</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 text-xs">
          <p className="text-muted-foreground">
            The cron dialect is the standard five fields,{" "}
            <em>minute hour day-of-month month day-of-week</em>, with day-of-week
            numbered 0–6 from Sunday. Timezone is an IANA name; the engine
            resolves it, including across daylight-saving transitions.
          </p>
          <CopyBlock
            text={`curl -X POST '${baseUrl}/schedules' \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \\
  -d '{"job_id":"'$JOB_ID'",
       "cron_expression":"0 2 * * 1-5",
       "timezone":"Asia/Kolkata",
       "misfire_policy":"FIRE_ONCE"}'`}
          />
          <p className="text-muted-foreground">
            Preview the next occurrences before committing to them, and read any
            daylight-saving anomalies the engine will report:
          </p>
          <CopyBlock
            text={`curl -H "Authorization: Bearer $TOKEN" \\
  '${baseUrl}/schedules/<id>/preview?count=5'`}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">4. Run it and watch</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3 text-xs">
          <CopyBlock
            text={`# Trigger immediately
curl -X POST "$BASE/jobs/$JOB_ID/trigger" \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{}'

# Follow it
curl -H "Authorization: Bearer $TOKEN" "$BASE/executions/<execution_id>"
curl -H "Authorization: Bearer $TOKEN" "$BASE/executions/<execution_id>/logs"
curl -H "Authorization: Bearer $TOKEN" "$BASE/executions/<execution_id>/timeline"`}
          />
        </CardContent>
      </Card>
    </div>
  );
}

function Concepts() {
  const concepts: { term: string; body: string }[] = [
    {
      term: "Job",
      body: "The thing you define. It holds no executable configuration of its own — that lives on versions.",
    },
    {
      term: "Version",
      body: "An immutable snapshot of a job's configuration. Publishing makes it the version new executions run against; older executions finish on the version they started with.",
    },
    {
      term: "Schedule",
      body: "A cron expression and timezone attached to a job. The scheduler claims due occurrences under a lease, so several schedulers can run without double-creating work.",
    },
    {
      term: "Execution",
      body: "One attempt to run a job version. Its status moves through queued, dispatched, running, and then a terminal state; every transition is validated against the state machine.",
    },
    {
      term: "Retry",
      body: "A failed execution can be retried. Each attempt is recorded, and the attempt count is what the concurrency policy and the retry budget are measured against.",
    },
    {
      term: "Lease",
      body: "A time-limited claim on a dispatched execution. A worker that stops heartbeating has its lease reaped and its execution recovered rather than stranded.",
    },
    {
      term: "Concurrency policy",
      body: "An optional bound on how many executions of a job, queue, or tenant may be active at once. It is enforced when work is dispatched, not only described.",
    },
    {
      term: "Idempotency",
      body: "Send an Idempotency-Key on any create or trigger. The same key with the same body replays the original response; the same key with a different body is a conflict.",
    },
    {
      term: "Pagination",
      body: "List endpoints return a page and a next cursor. Treat the cursor as opaque and pass it back untouched.",
    },
    {
      term: "Permission",
      body: "Roles carry permissions such as jobs:write. A 403 means the token's role lacks the permission; a 404 means the resource is not yours, which is deliberately indistinguishable from not existing.",
    },
    {
      term: "SLA",
      body: "An optional per-job target duration. Completed runs are measured against it, and compliance is a measurement rather than an estimate.",
    },
    {
      term: "Webhook",
      body: "An outbound subscription for domain events. Secrets are hashed on write and never returned again.",
    },
  ];

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">The vocabulary</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {concepts.map((concept) => (
          <div key={concept.term} className="flex flex-col gap-0.5">
            <span className="text-xs font-medium">{concept.term}</span>
            <span className="text-xs text-muted-foreground">{concept.body}</span>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}

function Recipes({ baseUrl }: { baseUrl: string }) {
  const recipes: { title: string; note: string; code: string }[] = [
    {
      title: "Make a request safe to retry",
      note: "Network failures make a retried POST land twice without this.",
      code: `curl -X POST "$BASE/jobs/$JOB_ID/trigger" \\
  -H "Authorization: Bearer $TOKEN" \\
  -H 'Idempotency-Key: settle-2026-10-03' \\
  -H 'Content-Type: application/json' -d '{}'`,
    },
    {
      title: "Paginate until exhausted",
      note: "Follow next_cursor until it comes back empty.",
      // A heredoc keeps the nested quoting readable, and avoids embedding a
      // backtick inside a template literal.
      code: [
        'CURSOR=""',
        'while :; do',
        '  URL="$BASE/executions?limit=100"',
        '  [ -n "$CURSOR" ] && URL="$URL&cursor=$CURSOR"',
        '  RESPONSE=$(curl -sH "Authorization: Bearer $TOKEN" "$URL")',
        '  echo "$RESPONSE" | python3 parse_page.py',
        '  CURSOR=$(echo "$RESPONSE" | python3 next_cursor.py)',
        '  [ -z "$CURSOR" ] && break',
        'done',
      ].join('\n'),
    },
    {
      title: "Ask why a job did not run",
      note: "The console's diagnosis logic, as an API call.",
      code: `curl -H "Authorization: Bearer $TOKEN" "$BASE/jobs/$JOB_ID/health"
curl -H "Authorization: Bearer $TOKEN" "$BASE/jobs/$JOB_ID/dependencies"
curl -H "Authorization: Bearer $TOKEN" "$BASE/schedules?target_id=$JOB_ID"`,
    },
    {
      title: "React to failures automatically",
      note: "Create a rule once and every matching failure reaches the alert inbox.",
      code: `curl -X POST "$BASE/alert-rules" \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \\
  -d '{"kind":"EXECUTION_FAILED","name":"settlement failures",
       "config":{"failures":3,"window_seconds":900},
       "cooldown_seconds":900}'`,
    },
    {
      title: "Hold scheduling during maintenance",
      note: "Nothing new is dispatched while it is on; work in flight is untouched.",
      code: `curl -X POST "$BASE/maintenance" \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \\
  -d '{"reason":"Upgrading the scheduler fleet"}'

# ... and lift it
curl -X DELETE "$BASE/maintenance" -H "Authorization: Bearer $TOKEN"`,
    },
    {
      title: "Reverse a mistake",
      note: "A job status change records its prior state and stays reversible.",
      code: `# Change it
curl -X PATCH "$BASE/jobs/$JOB_ID" \\
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \\
  -d '{"status":"ARCHIVED","expected_updated_at":"'"$UPDATED_AT"'"}'

# List what can be reversed, then reverse it
curl -H "Authorization: Bearer $TOKEN" "$BASE/undo"
curl -X POST "$BASE/undo/$ENTRY_ID" -H "Authorization: Bearer $TOKEN" -d '{}'`,
    },
    {
      title: "Find anything, quickly",
      note: "Grouped counts across jobs, executions, workers, and alerts.",
      code: `curl -H "Authorization: Bearer $TOKEN" "$BASE/search?q=status:FAILED"
curl -H "Authorization: Bearer $TOKEN" "$BASE/search?q=after:2026-09-01"`,
    },
    {
      title: "Generate a typed client",
      note: "The document is the contract; generation is your choice of tooling.",
      code: `# TypeScript
npx openapi-typescript "${API_URL}/openapi.json" -o forge.d.ts

# Python
pip install openapi-python-client
openapi-python-client generate --path "${API_URL}/openapi.json"`,
    },
  ];

  return (
    <div className="flex flex-col gap-3">
      {recipes.map((recipe) => (
        <Card key={recipe.title}>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">{recipe.title}</CardTitle>
            <p className="text-xs text-muted-foreground">{recipe.note}</p>
          </CardHeader>
          <CardContent className="pt-0">
            <CopyBlock text={recipe.code.replaceAll("$BASE", baseUrl)} />
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function WorkerSdks() {
  const sdks = [
    {
      language: "Python",
      description: "A lightweight SDK with auto-heartbeating and declarative @worker.job() decorators.",
      code: `from forge_sdk import ForgeWorker, JobContext

worker = ForgeWorker(base_url="http://localhost:3000/api/v1", tenant_id="tenant", api_key="key")

@worker.job("process-data")
def handle_process(ctx: JobContext):
    ctx.log("Processing...")
    return {"status": "SUCCESS"}

worker.start(queue="data-queue")`
    },
    {
      language: "Node.js (TypeScript)",
      description: "An Axios-based SDK for asynchronous execution.",
      code: `import { ForgeWorker, JobContext } from '@forge/sdk';

const worker = new ForgeWorker("http://localhost:3000/api/v1", "tenant", "key");

worker.job("process-data", async (ctx: JobContext) => {
    await ctx.log("Processing...");
    return { status: "SUCCESS" };
});

worker.start("data-queue");`
    },
    {
      language: "Go",
      description: "A highly concurrent implementation using goroutines.",
      code: `import "github.com/forge/sdk-go"

worker := forge.NewWorker("http://localhost:3000/api/v1", "tenant", "key")

worker.Register("process-data", func(ctx *forge.JobContext) (interface{}, error) {
    ctx.Log("Processing...")
    return map[string]string{"status": "SUCCESS"}, nil
})

worker.Start("data-queue", 2 * time.Second)`
    },
    {
      language: "Java",
      description: "A robust SDK leveraging Java 17 HttpClient and ExecutorService.",
      code: `import io.forge.sdk.*;

ForgeWorker worker = new ForgeWorker("http://localhost:3000/api/v1", "tenant", "key");

worker.registerJob("process-data", ctx -> {
    ctx.log("Processing...");
    return Map.of("status", "SUCCESS");
});

worker.start("data-queue", 2000);`
    }
  ];

  return (
    <div className="flex flex-col gap-4">
      {sdks.map((sdk) => (
        <Card key={sdk.language}>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">{sdk.language}</CardTitle>
            <p className="text-xs text-muted-foreground">{sdk.description}</p>
          </CardHeader>
          <CardContent className="pt-0">
            <CopyBlock text={sdk.code} />
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

/** A code block with a copy button, used throughout the documentation. */
function CopyBlock({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  return (
    <div className="relative rounded-md border border-border bg-muted/40">
      <pre className="overflow-x-auto p-3 pr-10 font-mono text-[11px] leading-relaxed">
        {text}
      </pre>
      <Button
        variant="ghost"
        size="icon"
        aria-label="Copy to clipboard"
        onClick={() => {
          navigator.clipboard?.writeText(text);
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        }}
        className="absolute right-1 top-1 size-7"
      >
        {copied ? (
          <Check className="size-3 text-emerald-600" aria-hidden />
        ) : (
          <Copy className="size-3" aria-hidden />
        )}
      </Button>
    </div>
  );
}
