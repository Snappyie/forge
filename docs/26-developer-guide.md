# Developer integration guide

How to integrate Forge into another project. This is the practical companion to
the API reference: the reference says what each endpoint accepts, this says what
to do and why.

- The machine-readable contract is `GET /api/v1/openapi.json`. It is generated
  from the same route table the server serves, and the build fails if the two
  disagree — so a client generated from it cannot drift.
- A runnable tour of everything below lives in
  [`examples/settlement-pipeline`](../examples/settlement-pipeline). Its
  `forge_demo/client.py` is a dependency-free reference client.
- The console renders the same material at `/docs`.

---

## 1. Concepts

Forge has five nouns. Understanding their separation is most of the integration.

### Job

The thing you define. A job is **inert**: it carries a name, a key, a priority,
and labels, but no executable configuration. A job is always in one of three
states — `DRAFT`, `ACTIVE`, `ARCHIVED`.

### Job version

An immutable snapshot of a job's configuration: what to run, its timeout, its
retry policy, and its concurrency policy. Versions exist so that a definition
can be reviewed before it runs, and so that a long-running execution finishes
against the version it started with even if you publish a new one meanwhile.

**A job cannot run until one of its versions is published.** This is the single
most common integration mistake, and it returns:

```json
{"error":{"code":"CONFLICT","message":"this job has no published version to run",
          "details":[{"field":"job","message":"publish a version first"}]}}
```

### Schedule

A cron expression, an IANA timezone, and a misfire policy, attached to a job or
a workflow. The scheduler claims due occurrences under a database lease, so you
can run several schedulers against one database without double-creating work.

### Execution

One attempt to run a job version. This is where all the runtime state lives.

### Lease

A time-limited claim on a dispatched execution. A worker that stops heartbeating
has its lease reaped and its execution recovered rather than stranded.

---

## 2. Authenticating

### The first account

Registration is closed by default. The very first registration on an empty
database claims the tenant and becomes its `OWNER`; it is a single, non-
repeatable claim. Every registration after that requires an invitation token,
and the endpoint answers `403 AUTHORIZATION_DENIED` without one.

```bash
# first account, once, on an empty database
curl -X POST http://localhost:3000/api/v1/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"you@example.com","password":"correct-horse-battery"}'
```

```bash
# every account afterwards
curl -X POST http://localhost:3000/api/v1/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"colleague@example.com","password":"correct-horse-battery",
       "invite_token":"8f3c...-uuid"}'
```

An owner creates invitations directly:

```sql
INSERT INTO invites (token, tenant_id, email, role, expires_at)
VALUES (gen_random_uuid(), '<tenant-uuid>', 'colleague@example.com',
        'DEVELOPER', NOW() + INTERVAL '7 days');
```

`invites.token` is a **UUID**, not an arbitrary string. A malformed token is a
`400`, not a database error.

### Tokens

A successful registration or login returns two tokens:

| Token | Lifetime | Use |
| --- | --- | --- |
| `access_token` | Short (hours) | `Authorization: Bearer` on every call |
| `refresh_token` | Long (days) | Exchanged for a new pair; rotates on use |

```bash
curl -X POST http://localhost:3000/api/v1/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"you@example.com","password":"correct-horse-battery"}'
```

```json
{"data":{"user_id":"…","tenant_id":"…","role":"OWNER",
         "access_token":"eyJ…","refresh_token":"…","expires_in_secs":3600}}
```

Refreshing **rotates both**. The previous refresh token is invalidated
immediately, and presenting it again is treated as theft: every token for that
session is revoked.

```bash
curl -X POST http://localhost:3000/api/v1/auth/refresh \
  -H 'Content-Type: application/json' \
  -d '{"refresh_token":"<the previous one>"}'
```

Refresh on a timer, not per request. A reasonable rule is to refresh when
`expires_in_secs` has less than a fifth remaining.

### Sending the token

```
Authorization: Bearer <access_token>
```

Not a cookie. That is deliberate: it means a cross-site request cannot ride an
authenticated session, so there is no CSRF surface to defend.

---

## 3. Authorization

Six roles, each a fixed set of permissions. A call outside the token's role
returns `403 AUTHORIZATION_DENIED`.

| Role | Permissions | Suits |
| --- | --- | --- |
| `OWNER` | all (`*`) | The first account; can delete the tenant |
| `ADMIN` | 22 | Day-to-day administration |
| `DEVELOPER` | 14 | Builds jobs, triggers them, reads executions |
| `OPERATOR` | 11 | Runs and supervises work, less configuration |
| `AUDITOR` | 8 | Reads everything, writes nothing operational |
| `VIEWER` | 5 | Dashboards only |

Permissions are `resource:action` pairs — `jobs:read`, `jobs:write`,
`executions:read`, `queues:read`, `settings:write`, `audit:read`, and so on. The
console hides actions the signed-in role cannot perform, and every endpoint
re-checks independently; hiding a button is a courtesy, not the enforcement.

### 403 versus 404, and why it matters

A request for a resource in another tenant returns **404, not 403**. A 403 would
confirm the resource exists, which is itself a leak: it turns the API into an
oracle for probing other tenants' identifiers. If you get a 404, treat it as
"not mine or not there" and do not retry with different identifiers.

---

## 4. Making writes safe to retry

Network failures make a retried `POST` land twice. Send an
`Idempotency-Key` on any create or trigger:

```bash
curl -X POST http://localhost:3000/api/v1/jobs/$JOB_ID/trigger \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Idempotency-Key: settle-2026-10-03' \
  -H 'Content-Type: application/json' -d '{}'
```

The behaviour is:

- same key, same body → the original response is replayed, no second execution
- same key, **different** body → `409 IDEMPOTENCY_KEY_CONFLICT`
- keys are scoped per tenant, so two tenants may use the same key

Scope the key to the thing it protects, not the request. `settle-2026-10-03` is a
good key for "run today's settlement once"; a random UUID per attempt is not,
because it deduplicates nothing.

---

## 5. Paginating

List endpoints return a page and a cursor:

```json
{"data":[ … ],
 "page":{"next_cursor":"eyJ…","has_more":true},
 "request_id":"…"}
```

```bash
curl -H "Authorization: Bearer $TOKEN" \
  'http://localhost:3000/api/v1/executions?limit=100'
```

```bash
curl -H "Authorization: Bearer $TOKEN" \
  'http://localhost:3000/api/v1/executions?limit=100&cursor=eyJ…'
```

- `limit` defaults to **50** and is capped at **200**.
- Treat the cursor as **opaque**. Pass it back byte-for-byte; do not parse it.
- Stop when `has_more` is `false` or `next_cursor` is `null`.

```python
cursor = None
while True:
    page = client.get("/executions", limit=100, cursor=cursor)
    for execution in page["data"]:
        handle(execution)
    if not page["page"]["has_more"]:
        break
    cursor = page["page"]["next_cursor"]
```

---

## 6. Cron and time

### The dialect

Five fields, standard order:

```
minute  hour  day-of-month  month  day-of-week
0       2     *             *      1-5
```

That example is 02:00 Monday through Friday.

**Day-of-week is numbered 0–6 from Sunday**, matching Unix `cron`. `1-5` is
Monday to Friday. `0` and `7` both mean Sunday.

Forge does not use the six-field form that includes seconds. `0 2 * * *` in
Forge means 02:00 daily; in a six-field system the same text would mean
"every two minutes".

### Timezone

Always set an explicit IANA timezone. Without one, the schedule is interpreted
in the server's default, which is rarely what you meant:

```json
{"target_id":"<job-uuid>","target_type":"JOB","expression":"0 2 * * 1-5",
 "schedule_type":"CRON","timezone":"Asia/Kolkata","misfire_policy":"FIRE_ONCE"}
```

### Daylight saving

The engine resolves local times against the zone, so a 02:00 job runs at 02:00
local on both sides of a transition. Two cases are unavoidable and are reported
rather than guessed:

- a **nonexistent** local time (spring forward skips it) — the occurrence is
  skipped
- an **ambiguous** local time (fall back repeats it) — the occurrence runs once

Preview a schedule to see which applies, rather than discovering it in
production:

```bash
curl -X POST http://localhost:3000/api/v1/schedules/$SCHEDULE_ID/preview \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{"count":10}'
```

The response lists the occurrences and any anomalies:

```json
{"data":{"occurrences":["2026-10-05T02:00:00Z", …],
         "anomalies":[{"at":"2026-11-01T02:30:00","kind":"AMBIGUOUS_LOCAL_TIME",
                       "note":"occurs twice; Forge runs it once"}]}}
```

### Misfire policy

If the scheduler was down when an occurrence was due, decide what happens:

| Policy | Behaviour |
| --- | --- |
| `FIRE_ONCE` | Run once, for the missed window. The default. |
| `SKIP` | Do not run the missed occurrence. |
| `CATCH_UP` | Replay missed occurrences, bounded by `catch_up_limit`. |

`FIRE_ONCE` is right for almost everything. `CATCH_UP` is for work where every
missed run matters, and you should set a limit so a long outage cannot produce a
thundering herd.

---

## 7. Running work

### Triggering

```bash
curl -X POST http://localhost:3000/api/v1/jobs/$JOB_ID/trigger \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Idempotency-Key: run-1' \
  -H 'Content-Type: application/json' -d '{}'
```

Returns `202` with the new execution's identifier and status.

### Following an execution

```bash
curl -H "Authorization: Bearer $TOKEN" "$BASE/executions/$EXEC_ID"
curl -H "Authorization: Bearer $TOKEN" "$BASE/executions/$EXEC_ID/timeline"
curl -H "Authorization: Bearer $TOKEN" "$BASE/executions/$EXEC_ID/logs"
curl -H "Authorization: Bearer $TOKEN" "$BASE/executions/$EXEC_ID/metrics"
```

The timeline is built from timestamps that exist, so a stage that has not
happened yet is absent rather than shown at zero elapsed.

### Statuses

```
SCHEDULED → QUEUED → DISPATCHED → RUNNING → SUCCEEDED
                                                  ↘ FAILED / TIMED_OUT → RETRY_SCHEDULED → QUEUED
                                                  ↘ DEAD_LETTERED
             CANCELLED ◄── CANCEL_REQUESTED
```

- `ABANDONED` means a lease expired and recovery started. It is **not** terminal:
  the execution is requeued, so a crashed worker does not lose work.
- Every transition is validated against this state machine. An illegal
  transition is refused rather than stored.

### What is not there

Some reads are honest about absence rather than inventing data:

- **Logs** are empty until a worker writes them. An empty list is not an error.
- **Metrics** return `sampled: false` when no worker has posted resource
  samples. The console shows "no data" rather than a flat zero line.
- **Job health** returns `null` for a percentile that has not been measured, and
  `success_rate` is `null` — not `0%` — when nothing has run at all.

Trust those nulls. Converting them to zero is how dashboards start lying.

---

## 8. Retrying and failure

### Retry policy

Attached to a version:

```json
{"config":{"url":"https://example.com/run"},
 "retry_policy":{"max_attempts":3,"backoff":"exponential",
                 "initial_delay_ms":1000,"max_delay_ms":300000},
 "timeout_seconds":300}
```

An execution's `attempt_count` is what the retry budget and the concurrency
policy are both measured against.

### Retry policy fields

```json
{"retry_policy":{
   "max_attempts":3,
   "backoff":{"Exponential":{"initial_delay":{"secs":1,"nanos":0},
                             "multiplier":2,
                             "max_delay":{"secs":300,"nanos":0}}},
   "retryable_error_classes":["TRANSIENT","TIMEOUT"],
   "non_retryable_error_classes":["VALIDATION"],
   "jitter_ratio":0.2}}
```

| Field | Effect |
| --- | --- |
| `max_attempts` | Total attempts, not retries after the first |
| `backoff` | A tagged object; see below |
| `retryable_error_classes` | Classes eligible for retry; empty means the class defaults |
| `non_retryable_error_classes` | Explicit exclusions, overriding the allow-list |
| `jitter_ratio` | Fraction of the delay jitter may add, `0.0`–`1.0` |

### Backoff strategies

Each variant is a **struct variant with its own fields**, so it is nested rather
than a bare string.

| Variant | Fields |
| --- | --- |
| `{"Fixed": {"delay": {...}}}` | One constant delay |
| `{"Linear": {"initial_delay": {...}, "increment": {...}}}` | Grows by a fixed step |
| `{"Exponential": {"initial_delay": {...}, "multiplier": N, "max_delay": {...}}}` | Multiplies, capped |

### Durations are objects, not strings

A duration is `{"secs": 1, "nanos": 0}` — **not** `"PT1S"`. An ISO-8601 string is
rejected with `expected struct Duration`.

| Seconds | Encoding |
| --- | --- |
| 1 second | `{"secs": 1, "nanos": 0}` |
| 30 seconds | `{"secs": 30, "nanos": 0}` |
| 5 minutes | `{"secs": 300, "nanos": 0}` |

A bare `"Exponential"` without its fields is likewise rejected, with
`expected struct variant`. Both are validation errors that name what was
expected, so a mistake is reported rather than silently defaulted.

The exclusion list wins over the allow-list. That ordering matters: it lets you
say "retry anything transient" and then carve out one case without restating the
whole list.

### Jitter

Without jitter, every execution that failed at the same instant retries at the
same instant, which turns a partial outage into a synchronised stampede.
`jitter_ratio` randomises each delay by up to that fraction of its computed
value. Anything above `0` is worth setting in production.

### Retrying by hand

```bash
curl -X POST http://localhost:3000/api/v1/executions/$EXEC_ID/retry \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{}'
```

The execution moves to `RETRY_SCHEDULED` and a background pass requeues it.
It is not dispatched inline, so a retry follows the same scheduling rules as any
other work.

### Attempt history

```bash
curl -H "Authorization: Bearer $TOKEN" "$BASE/executions/$EXEC_ID/attempts"
```

Every attempt with its status, timestamps, and error — so an operator can see
that attempt 1 timed out and attempt 2 was rejected for a different reason.

### Error classes

`error_class` is a closed set, not free text:

`VALIDATION`, `AUTHENTICATION`, `AUTHORIZATION`, `NOT_FOUND`, `CONFLICT`,
`RATE_LIMITED`, `TRANSIENT`, `DEPENDENCY_UNAVAILABLE`, `TIMEOUT`,
`CANCELLATION`, `RESOURCE_EXHAUSTED`, `PERMANENT`, `INTERNAL`.

Branch on the class, not the message: messages are written for humans and may
change; classes are part of the contract.

`TRANSIENT` and `DEPENDENCY_UNAVAILABLE` are worth retrying. `PERMANENT` is not —
retrying a malformed request just wastes attempts.

---

## 9. Concurrency

An optional bound, set on the **version** rather than the job, so two versions
of the same job can be constrained differently.

```json
{"concurrency_policy":{
   "max_concurrent_executions":{"Bounded":2},
   "scope":"JOB",
   "queue_id":null}}
```

### Fields

| Field | Values | Meaning |
| --- | --- | --- |
| `max_concurrent_executions` | `{"Bounded": N}` or `{"Unlimited"}` | The cap. `Unlimited` is the default |
| `scope` | `JOB`, `QUEUE`, `TENANT`, `GLOBAL`, `WORKFLOW` | Which executions count against the cap |
| `queue_id` | UUID or `null` | Only meaningful for `QUEUE` scope |

`{"Bounded": N}` is the **only** accepted encoding. `{"max": N}` is rejected
with a validation error, as is a bare number.

### What counts as active

An execution counts against the limit while it is in any of:

```
SCHEDULED, QUEUED, DISPATCHED, RUNNING, RETRY_SCHEDULED,
CANCEL_REQUESTED, ABANDONED
```

That includes `QUEUED`. A job bounded to 1 that has one execution waiting will
**not** start a second — which is the point: the bound is on how much work is in
flight for that scope, not merely on how much is running.

It also includes `ABANDONED`, which is a recovery state and not terminal, so a
crashed worker's execution keeps occupying its slot until recovery requeues or
finishes it.

### Where it is enforced

At dispatch. When a worker claims work, the candidate execution is only
selected if its scope is under the limit. A job bounded to 2 will not take a
third execution whatever the queue depth looks like.

This is enforced in the same query that selects work, so it cannot be bypassed
by adding more schedulers or more workers.

### Choosing a scope

- **`JOB`** — the common case. One job at a time, regardless of how many
  others run.
- **`TENANT`** — a ceiling on the whole tenant, useful as a safety valve.
- **`QUEUE`** — per-queue capacity. Pair it with `queue_id`; without it the
  scope cannot be evaluated.
- **`WORKFLOW`** and **`GLOBAL`** — reserved; the workflow engine and the
  process-wide counter apply them.

### Seeing the effect

`GET /jobs/{id}/health` reports `executions` alongside the success rate, so a
job sitting at its ceiling looks like a backlog rather than a mystery.

---

## 10. SLA targets

An SLA is an **expected duration**. Forge measures completed runs against it and
reports what actually happened.

```bash
curl -X PUT http://localhost:3000/api/v1/jobs/$JOB_ID/sla \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' -d '{"target_duration_seconds":1800}'
```

| Field | Type | Notes |
| --- | --- | --- |
| `target_duration_seconds` | integer > 0 | Seconds a completed run is expected to take |
| `enabled` | boolean | Defaults to true; disable without deleting |

`0` or a negative value is a `400`, not a stored nonsense target.

### Reading compliance

```bash
curl -H "Authorization: Bearer $TOKEN" "$BASE/jobs/$JOB_ID/health"
```

```json
{"sla":{"target_seconds":1800,"met":4,"evaluated":6,"compliance_percent":66.7}}
```

- `compliance_percent` is `null` until something has been evaluated. Treat "no
  evaluation yet" as unknown, **not** as a pass.
- The target is measured from `started_at` to `ended_at`. Queue wait is not
  counted against the SLA; a job that waited an hour for a worker and then ran
  quickly has met its target.
- Only completed runs are evaluated. An execution still running is not a miss.

### Per-run detail

```bash
curl -H "Authorization: Bearer $TOKEN" "$BASE/jobs/$JOB_ID/sla"
```

Returns each evaluated execution with its duration and whether it met the
target, so you can show an operator *which* runs were late rather than only a
percentage.

### Tenant-wide

```bash
curl -H "Authorization: Bearer $TOKEN" "$BASE/sla/compliance"
```

Counts evaluations from the last 30 days, which is what the dashboard's SLA
card reads.

---

## 11. Alerts

Create a rule once; it then evaluates continuously:

```bash
curl -X POST http://localhost:3000/api/v1/alert-rules \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"kind":"EXECUTION_FAILED","name":"settlement failures",
       "config":{"failures":3,"window_seconds":900},
       "cooldown_seconds":900}'
```

`cooldown_seconds` matters: without it a job failing in a tight loop can raise
an alert on every attempt.

Kinds: `EXECUTION_FAILED`, `SLA_VIOLATION`, `QUEUE_BACKLOG`, `WORKER_OFFLINE`,
`LATENCY_SPIKE`, `SCHEDULE_MISSED`.

Alerts move `OPEN → ACKNOWLEDGED → RESOLVED`. Acknowledging twice returns 404:
the second call finds nothing open.

---

## 12. Webhooks

```bash
curl -X POST http://localhost:3000/api/v1/webhooks \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"ops-alerts","url":"https://example.com/forge-events",
       "auth_kind":"HMAC","secret":"<stored hashed, never returned again>",
       "events":["execution.failed","execution.succeeded"],
       "max_retries":5,"timeout_seconds":10}'
```

**Destination addresses are checked against the SSRF rules** before storage:
loopback, private ranges, link-local, and cloud metadata endpoints are refused.
A webhook pointing at `169.254.169.254` is a mistake, or an attack; either way it
is refused.

Use `Test webhook` to queue a delivery and read back the recorded outcome. The
response reports what actually happened — Forge does not dial external
endpoints on demand, so a "verified" connection would be a fiction.

---

## 13. Workflows

A workflow is a DAG of typed nodes. Node types: `JOB`, `APPROVAL`, `DELAY`,
`CONDITION`, `MAP`, `WEBHOOK`.

```bash
curl -X POST http://localhost:3000/api/v1/workflows \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"Nightly close","key":"nightly-close",
       "definition":{"nodes":[
          {"key":"extract","name":"Extract movements","type":"JOB",
           "config":{"job_id":"<uuid>"}},
          {"key":"approve","name":"Finance approval","type":"APPROVAL","config":{}}],
         "edges":[{"from":"extract","to":"approve"}]}}'
```

A cyclic graph is rejected on save — a cycle would never finish. Saving an
edited graph creates a **new draft version** rather than mutating the published
one, so a running workflow is never changed underneath itself.

---

## 14. Maintenance mode

Hold scheduling tenant-wide while you do work that new runs would interfere
with — a migration, a fleet upgrade, a noisy neighbour.

```bash
# enter
curl -X POST http://localhost:3000/api/v1/maintenance \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"reason":"Upgrading the scheduler fleet"}'

# read the current state
curl -H "Authorization: Bearer $TOKEN" "$BASE/maintenance"
```

```json
{"data":{"active":true,
         "window":{"id":"…","reason":"Upgrading the scheduler fleet",
                   "started_at":"2026-10-04T05:00:00Z","started_by":"…"}}}
```

```bash
# lift
curl -X DELETE http://localhost:3000/api/v1/maintenance \
  -H "Authorization: Bearer $TOKEN"
```

### What it does and does not stop

| Effect | Behaviour |
| --- | --- |
| New scheduled occurrences | Not created while maintenance is active |
| Queued executions | **Still dispatch** when a worker claims them |
| Running executions | **Not interrupted**; they finish normally |
| Manual triggers | Still work |
| Lifting | `DELETE` closes the window; scheduling resumes |

That boundary is deliberate. Maintenance is a scheduling brake, not a kill
switch. To stop work that is already in flight, cancel it explicitly — see
`POST /emergency/cancel-running` in section 22.

### Why a reason is required

An empty or whitespace reason is a `400`. The reason is shown on the dashboard
banner, so anyone looking at a stalled system can see why it was paused without
asking.

### Lifting a window that is not open

`DELETE` on a tenant with no open window returns `404`, rather than succeeding
silently. That makes it safe to call unconditionally in a shutdown script.

### If maintenance is left on

The window stays open until lifted. There is no automatic expiry, because a
maintenance window that silently closed mid-upgrade would be worse than one that
stays. The dashboard banner is deliberately unmissable for exactly this reason.

---

## 15. Undo

A job status change records its prior state, so a mistaken pause or archive is one
click to reverse rather than a second hand-edited request.

```bash
# what can be reversed
curl -H "Authorization: Bearer $TOKEN" "$BASE/undo"

# reverse it
curl -X POST "$BASE/undo/$ENTRY_ID" -H "Authorization: Bearer $TOKEN" -d '{}'
```

```json
{"data":{"undone":true,"action":"JOB_STATUS",
         "resource_id":"…","restored_status":"ACTIVE"}}
```

### The rules

| Rule | Consequence |
| --- | --- |
| 24-hour window | An entry past `expires_at` is no longer listed |
| Single use | A second attempt returns `404`; the entry is consumed |
| Per user | You only see and reverse your own changes |
| Per tenant | An entry cannot be undone from a different tenant |
| Job status only | Other actions are refused explicitly, not silently ignored |

### What is reversible

Currently `JOB_STATUS` changes — whether made through `PATCH /jobs/{id}` or a
bulk pause, resume, or archive. Everything else returns a validation error
naming the action, rather than pretending to reverse something it did not.

### When no entry appears

The change was a no-op. Pausing a job that is already `DRAFT` records nothing,
because nothing changed — so an empty undo list after a bulk call usually means
the jobs were already in the requested state, not that undo is broken.

---

## 16. Search and the assistant

```bash
curl -H "Authorization: Bearer $TOKEN" "$BASE/search?q=settlement"
curl -H "Authorization: Bearer $TOKEN" "$BASE/search?q=status:FAILED"
curl -H "Authorization: Bearer $TOKEN" "$BASE/search?q=after:2026-09-01"
```

Results are grouped with per-group counts. A prefix Forge does not implement
falls back to a literal text search rather than silently returning nothing.

The assistant answers from stored data and **proposes** configuration changes
without applying them:

```bash
curl -X POST "$BASE/assistant/ask" -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"question":"What jobs are running right now?"}'

curl -X POST "$BASE/assistant/propose" -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"question":"Create a job that runs every weekday at 2am"}'
```

The proposal returns `applied: false` and a cron expression. **Nothing changes
until a human applies it.** If the assistant does not recognise a question it
says so and lists what it can do, rather than improvising an answer.

---

## 17. Errors

Every error shares one envelope:

```json
{"error":{"code":"VALIDATION_ERROR",
          "message":"email must be a valid address",
          "details":[{"field":"email","message":"invalid"}],
          "request_id":"9f1c…"},
 "request_id":"…"}
```

`request_id` is the one field worth logging: it appears in the server's log
lines, so a user reporting "it failed" hands you the exact request.

| Code | Status | Meaning | Retry? |
| --- | --- | --- | --- |
| `VALIDATION_ERROR` | 400 | Malformed input | No — fix the request |
| `AUTHENTICATION_REQUIRED` | 401 | Missing or expired token | After refreshing |
| `AUTHORIZATION_DENIED` | 403 | Role lacks the permission | No |
| `NOT_FOUND` | 404 | Absent, or another tenant's | No |
| `CONFLICT` | 409 | The request conflicts with current state | No |
| `IDEMPOTENCY_KEY_CONFLICT` | 409 | A key was reused with a different body | No — use a new key |
| `RATE_LIMITED` | 429 | Too many requests | Yes, after the stated delay |
| `INTERNAL_ERROR` | 500 | Server fault | Yes, with backoff |

**Retry policy:** retry `RATE_LIMITED` and `5xx` with exponential backoff and
jitter. Do not retry `4xx` other than 429 — the request is wrong, and repeating
it wastes the budget.

---

## 18. Generating a client

The document is the contract. Generation is your choice of tooling:

```bash
# TypeScript
npx openapi-typescript http://localhost:3000/api/v1/openapi.json -o forge.d.ts

# Python
pip install openapi-python-client
openapi-python-client generate --path http://localhost:3000/api/v1/openapi.json

# Go
oapi-codegen -generate types,client -package forge \
  http://localhost:3000/api/v1/openapi.json > forge.gen.go
```

Public operations carry `security: []`; everything else inherits the bearer
scheme. Generating a client from the document therefore gives you a typed method
per endpoint, with the unauthenticated ones correctly marked.

---

## 19. Configuration

Security-critical settings fail closed: Forge **refuses to start** without them.

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `FORGE_DATABASE_URL` | no | local `forgedb` | Postgres connection string |
| `FORGE_AUTH_SESSION_SECRET` | **yes** | — | Signs session tokens |
| `FORGE_API_KEY_HASHING_SECRET` | **yes** | — | Hashes API keys |
| `FORGE_ALLOW_OPEN_REGISTRATION` | no | `false` | Invites still apply |
| `FORGE_RATE_LIMIT_PER_SECOND` | no | 100 | Per-client request rate |
| `FORGE_RATE_LIMIT_BURST` | no | 200 | Burst allowance |

Set `FORGE_ALLOW_OPEN_REGISTRATION=true` only where self-service signup is
intended, and understand that open signups receive `VIEWER`, not `OWNER`.

---

## 20. Checklist for a first integration

1. Start the server; confirm `GET /api/v1/health/live` returns `ok`.
2. Register the first account; note the tenant and the access token.
3. `GET /api/v1/system/health` and read every row. `unknown` means unmeasured,
   not healthy.
4. Create a job. Confirm it comes back `DRAFT`.
5. Create a version and **publish** it.
6. Trigger it with an `Idempotency-Key`; read the execution back.
7. Attach a schedule with an explicit timezone; preview it.
8. Set an SLA target so compliance becomes measurable.
9. Add an alert rule, and a webhook if you need to be told.
10. Run [`examples/settlement-pipeline/demo.py`](../examples/settlement-pipeline)
    end to end as a smoke test of the whole surface.

If step 10 passes, your integration understands everything Forge does.

## 6. Worker SDKs

Forge provides official SDKs to simplify distributed job execution in your favorite language. 
The SDKs automatically manage queue polling, execution context parsing, long-running heartbeats, and log streaming so that you can focus entirely on business logic.

### Python

```python
from forge_sdk import ForgeWorker, JobContext

worker = ForgeWorker(base_url="http://localhost:3000/api/v1", tenant_id="tenant", api_key="key")

@worker.job("process-data")
def handle_process(ctx: JobContext):
    ctx.log("Processing...")
    return {"status": "SUCCESS"}

worker.start(queue="data-queue")
```

### Node.js (TypeScript)

```typescript
import { ForgeWorker, JobContext } from '@forge/sdk';

const worker = new ForgeWorker("http://localhost:3000/api/v1", "tenant", "key");

worker.job("process-data", async (ctx: JobContext) => {
    await ctx.log("Processing...");
    return { status: "SUCCESS" };
});

worker.start("data-queue");
```

### Go

```go
import "github.com/forge/sdk-go"

worker := forge.NewWorker("http://localhost:3000/api/v1", "tenant", "key")

worker.Register("process-data", func(ctx *forge.JobContext) (interface{}, error) {
    ctx.Log("Processing...")
    return map[string]string{"status": "SUCCESS"}, nil
})

worker.Start("data-queue", 2 * time.Second)
```

### Java

```java
import io.forge.sdk.*;

ForgeWorker worker = new ForgeWorker("http://localhost:3000/api/v1", "tenant", "key");

worker.registerJob("process-data", ctx -> {
    ctx.log("Processing...");
    return Map.of("status", "SUCCESS");
});

worker.start("data-queue", 2000);
```
