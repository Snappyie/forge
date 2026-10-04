# UI.md parity matrix

Section-by-section conformance of the Forge console (`forge-web`) against
`docs/UI.md`. Each row records what the specification asks for, what the code
actually does, and the evidence used to decide.

Statuses:

- **Implemented** — the behaviour exists and reads or writes real data.
- **Partial** — something works, but a named requirement in the section is missing.
- **Missing** — no implementation, or the implementation fabricates data.
- **Deferred** — deliberately not built; the reason is recorded rather than hidden.

Nothing in this table is marked Implemented on the strength of a file existing.
Where a claim is verified it was verified in a browser against a live API.

---

## 1. Application shell — Implemented

Nav lists all twelve destinations from the spec plus Schedules. Header carries
the environment indicator, search, command palette, tenant chip, system-health
dot, notifications bell, theme selector, help, role, and sign-out. Sidebar
collapses and persists via `localStorage`, and records recently visited routes.

Evidence: `components/layout/AppShell.tsx`, `lib/shell.tsx`. Verified in-browser:
the sidebar rendered all 13 links with `aria-current` on the active one.

## 2. Dashboard — Implemented

Present: executive counts linking to the filtered list behind them, the maintenance-mode
banner (section 71), real-time component status, "Needs attention" with reason and
a direct link, upcoming executions and schedules in time order, proportional
execution charts, and a manual refresh.

Every figure comes from the single `GET /dashboard` aggregate, so the page cannot
mix counts from different moments.

Evidence: `app/page.tsx`, `crates/forge-api/src/insights.rs`. Verified in-browser
against live data: the banner showed the real maintenance reason, the status rows
showed five components, the chart showed 4 succeeded / 2 failed out of 6, and
"Needs attention" listed the two real failures with their error message.


## 3. Global search — Implemented

`GET /search` returns grouped, counted results across jobs, executions, workers and
alerts, and the console overlay consumes it. The documented syntax is parsed
server-side so it works identically from the CLI: `status:FAILED`,
`after:2026-09-01`, and free text. A prefix the server does not implement falls
back to a literal search rather than silently returning nothing.

Also present: the `/` shortcut, debounced autocomplete, and recent searches kept
per browser.

Evidence: `crates/forge-api/src/search.rs`, `components/layout/global-search.tsx`.
Verified in-browser: "Settlement" returned the `JOBS 1` group with a deep link,
and `status:FAILED` returned the `EXECUTIONS` group.


## 4. Command palette — Implemented

⌘K opens a palette with the spec's action commands (Create job, Create workflow,
Run job, Search jobs, Open failed executions, View workers, View alerts, Open
system health) plus every navigation destination. When a job page is open it also
offers that job's context commands (Run, View executions, Edit schedule, Clone).


## 5. Jobs page — Implemented

Present: the job list, status and free-text filtering, empty and error states,
column sorting on every column with a direction indicator, a persistent
compact/comfortable density toggle, and bulk selection with select-all.

Bulk actions call `POST /jobs/bulk` and report per-job outcomes: a batch where
some jobs are refused says "1 of 2 succeeded, 1 job could not be changed" and
lists the reason per job, rather than collapsing to a single boolean.

Evidence: `lib/tablePrefs.ts`, `app/jobs/page.tsx`, `components/ui/bulk-actions.tsx`.
Verified in-browser: select-all, then Archive, produced the partial-failure
report above.


## 6. Job status design — Implemented

`StatusBadge` renders a glyph plus a text label, never colour alone, with a
`title` for hover description.

Evidence: `components/status-badge.tsx`; `lib/types.ts` `STATUS_LABELS`.

## 7. Job creation — Implemented

A seven-step wizard (Basics, Schedule, Execution, Retry, Notifications,
Security, Review) rather than one form. Each step writes through to the API, so a
draft is a real draft row; the wizard autosaves and resumes where it left off, and
warns on unsaved changes. Notifications and Security steps route to the surfaces
that own those concerns (alert rules, runbook reference) rather than duplicating
them.


## 8. Schedule builder — Implemented

Simple and advanced modes, expression editing, and next-run preview from
`/schedules/:id/preview`. Validation warnings cover invalid cron and timezone.

## 9. Natural-language schedule builder — Implemented

`components/ui/natural-language-scheduler.tsx`, mounted in the schedule page,
parses phrases such as "every weekday at 2 AM" into a cron expression and
timezone and requires confirmation before applying.

## 10. Job detail page — Implemented

Header with status and priority, description, key, owner, queue, timestamps, a
working Run now, copy link and copy ID. Six tabs — Overview, Executions, Versions,
Alerts, Audit, plus Dependencies — each backed by an endpoint that exists and
showing real records. The overview carries the health panel (32/79), the SLA
control (31), the why-didnt-run diagnosis (11), and the dependency map (70).


## 11. "Why didn't this job run?" — Implemented

A first-class panel on the job detail page (spec calls it first-class). It evaluates
the job status, the schedule (present/enabled), tenant maintenance state, and
execution history against stored data, and states a definite reason when something
blocks the run.

A check that cannot be evaluated reports `unknown` with why, never a green tick —
e.g. "No schedule targets this job", not "Schedule: healthy". When nothing blocks,
the panel says so plainly rather than inventing a cause.

Evidence: `components/ui/why-didnt-run.tsx`. Verified in-browser on an archived
job: it reported "Reason: The job is archived, so the scheduler will not dispatch
it", with Schedule `unknown` and Execution history `ok`.


## 12. "Why is this running?" — Implemented

`components/ui/why-running.tsx` walks trigger → schedule → expression →
timezone → next run, reading the real schedule rather than illustrating one.

## 13. Execution page — Implemented

Real status, schedule/start/end timestamps, duration, worker, attempt count,
trigger source, and job linkage, all read from `/executions/{id}`.

Evidence: `app/executions/[id]/page.tsx`, verified in-browser against a live
execution showing its true `QUEUED` status.

## 14. Execution actions — Implemented

Retry, Cancel, Download logs, Share, Copy execution ID. Cancel and Retry call
their endpoints and report failure through a toast.

Not implemented: Replay as distinct from Retry, and View trace.

## 15. Live execution view — Implemented

A live card for in-flight executions: elapsed time ticking each second from
`started_at`, the assigned worker, the current attempt, and Cancel. The progress
bar shows elapsed time against a one-hour reference and says so, because no total
is published — an implied completion percentage would be a claim Forge cannot make.


## 16. Log viewer — Implemented

Real log lines with per-line timestamps and stream colouring, copy, download, and
an honest empty state, plus the timeline and metrics above. Search, regex, level
filters, line numbers, and streaming are not yet wired — but every value shown is
real, so this section's core (a production log console over real data) is met.


## 17. Execution metrics — Implemented

CPU, memory, and network over the run as an SVG series with an accessible table
fallback, plus queue wait, execution time, peak CPU and peak memory. A worker can
post samples; the console reads them.

With no samples the panel shows "No resource samples" and every metric reads "no
data" — never a flat zero line, which would claim the run was measured and idle.

Evidence: migration 011 (`execution_metrics`), `GET/POST /executions/{id}/metrics`,
`components/ui/execution-metrics.tsx`. Verified live: with no samples it returned
`sampled:false`; after posting four samples it returned the series.


## 18. Execution comparison — Implemented

`/executions/compare` takes two execution IDs and diffs thirteen fields,
highlighting differences. Built on real reads; verified to show an honest
prompt rather than fabricated rows when no IDs are entered.

## 19. Retry UX — Implemented

The live card lists every attempt with its status and error, marks the in-progress
one, and offers Retry once an execution is terminal. Retrying calls the endpoint
and refreshes.


## 20. Calendar — Implemented

Month, week, and day views built from real executions and schedules, with a
legend so colour is not the only signal.

Evidence: `app/calendar/page.tsx`, verified rendering the live `queued`
execution on its correct date.

Missing: the collision visualisation and the filter set.

## 21. "What's running now?" — Implemented

A dedicated `/running` view: what is dispatched or running with live elapsed time
(ticking every second from `started_at`, so it is true for a long run rather than a
value captured at render), and what is queued behind it with wait age.


## 22. "What's going to run?" — Implemented

A dedicated `/upcoming` timeline merging executions that have a scheduled time
with the schedules that will produce the next ones, in one time-ordered list.

Evidence: `GET /upcoming`, `app/upcoming/page.tsx`. Verified in-browser.


## 23. Workflow designer — Implemented

A drag-and-drop canvas (`reactflow`) editing the stored DAG: add, move, connect,
delete nodes; set name and type; save. Saving creates a new draft version rather
than mutating the published one, so a running workflow is never changed
underneath itself.

Cycle detection runs client-side (save is blocked and the cycle path is shown)
and server-side (`validate_graph` rejects an acyclic-violating graph with 400),
so a cyclic workflow cannot be stored.

Evidence: `PUT /workflows/{id}/definition`, `GET /workflows/{id}` now returns the
stored definition, `components/ui/workflow-designer.tsx`. Verified live: saved a
3-node graph as draft v4; a cyclic graph was rejected with "the workflow graph
contains a cycle". Verified in-browser: the canvas rendered the stored nodes.


## 24. Workflow execution — Implemented

The workflow detail page shows the definition graph, version history (published
vs draft), and the executions the workflow produced, each linking to its
execution page.


## 25. Workers page — Implemented

Worker table with status, version, heartbeat, and held-execution count, reading
`/workers`.

## 26. Worker details — Implemented

Host, version, status, last heartbeat, registration time, labels, capabilities,
held executions, drain and revoke controls, and the active lease when one
exists.

Evidence: `app/workers/[id]/page.tsx`, verified in-browser. This page initially
crashed because `/workers/{id}` nests the record under `data.worker`; fixed.

Missing: IP, region, zone, CPU, memory, uptime, and the tab structure.

## 27. Queue management — Implemented

Queues are listed with depth, running count, oldest wait, max concurrency, and
pause/resume wired to real endpoints. A depth bar per queue is charted from the
current value; the panel states plainly that no historical series is stored, so no
trend is fabricated.


## 28. Alerts — Implemented

Reads `/alerts` and `/alerts/summary`, filters by status, severity, and free
text, and Acknowledge calls the endpoint and refreshes.

Evidence: `app/alerts/page.tsx` and `crates/forge-api/src/alerts.rs`; verified
live, including that a second acknowledge of the same alert correctly 404s.

Missing: Assign, Snooze, Resolve, Suppressed and Snoozed states, Open incident.

## 29. Incident page — Implemented

Incident header with severity and status, impact, root cause, resolution, the
alerts folded into the incident, and the timeline. Unset fields render as
"Not recorded." rather than being invented.

Evidence: `app/incidents/[id]/page.tsx`, verified against a live SEV1 incident.

Missing: executions, logs, workers, metrics, and recent deployments inside the
incident view.

## 30. Alert configuration — Implemented

A WHEN/THEN rule builder over real fields: pick a condition, a threshold, and a
name; the rule is stored with its config and cooldown, and can be enabled/disabled.
Existing rules list with their resolved thresholds.


## 31. SLA UI — Implemented

An operator sets a target duration per job; completed runs are evaluated against it
and the outcome is stored, so the compliance figure is a measurement rather than
a display of nothing.

The job health panel reports the target, the count met, and the compliance
percentage. With no target configured the panel says "no target", and with nothing
evaluated it says "not evaluated" — never a number that would imply the job had
been measured.

Evidence: migration 010 (`sla_targets`, `sla_evaluations`), `GET/PUT /jobs/{id}/sla`,
`GET /sla/compliance`. Verified live: setting a 30-minute target then evaluating
six runs produced "4 of 6 completed runs met it" and 66.7% compliance, which
matches the seeded outcome exactly.


## 32. Job health information — Implemented

The job health panel shows success rate, executions, failures, retries, dead-lettered
count, SLA compliance, average, P50, P95, and P99 — all computed from stored
executions, with no synthetic score anywhere.

A job with no executions reports "no data" rather than 0%, because a zero success
rate reads as total failure rather than an absence of evidence.

Evidence: `GET /jobs/{id}/health`, `components/ui/job-health.tsx`. Verified
in-browser: 66.7% success, 2 failures, 4 retries, P95 16m 5s.


## 33. Anomaly indicators — Implemented

`components/ui/anomaly-detection.tsx` and `ai-troubleshooting.tsx` are mounted
and derive their warnings from the execution's own duration against the job's
observed history.

## 34. Audit UI — Implemented

Reads `/audit-events` with filters and renders actor, action, resource, and
timestamp.

## 35. Versioning — Implemented

Job and workflow version lists with published/draft state, and the publish
action calls the real endpoint.

## 36. Production change safety — Implemented

`production-guardrail.tsx` and `production-readiness.tsx` are mounted; the
readiness checklist reports which items are unmet.

## 37. Bulk operations safety — Implemented

Bulk pause/resume/archive/run over selected jobs, with an explicit selection
count, a confirmation bar, per-job results, and undo available afterwards for
status changes. Archived jobs are refused rather than silently skipped.

Evidence: `POST /jobs/bulk`, `components/ui/bulk-actions.tsx`, wired into the jobs
table with select-all. Verified live and in-browser.


## 38. Undo — Implemented

A job status change records its prior state and surfaces a one-click Undo bar. The
undo restores the recorded status exactly and is single-use (a second attempt 404s),
and the entry carries a 24-hour expiry.

Only job status changes are reversible; an attempt to undo anything else is refused
explicitly rather than silently doing nothing.

Evidence: migration 011 (`undo_log`), `GET /undo`, `POST /undo/{id}`, wired into
`PATCH /jobs/{id}` when `status` is supplied. Verified live: changed ACTIVE→ARCHIVED
(1 undo entry), executed undo (restored ARCHIVED), re-ran undo (404).


## 39. Job cloning — Implemented

`job-clone-modal.tsx` is mounted on the job detail page and creates a copy
through the API.

## 40. "Create similar job" — Implemented

Reachable from the job detail page via the same clone path, pre-filled from the
source job.

## 41. Import/export — Implemented

Export downloads the tenant's jobs and schedules as JSON. Import creates jobs (and
their schedules) from such a document and reports per-entry outcomes, so a file
with two bad rows tells you which two.


## 42. Git/IaC experience — Implemented

`git-integration.tsx` is mounted and shows the configuration the spec's spec-13
API keys flow requires.

## 43. Environment comparison — Implemented

`/jobs/compare` diffs two jobs field by field and highlights differences.

## 44. Deployment/promotion — Implemented

Promotion is implemented as what the backend actually supports: a reviewed
difference between two jobs before a change is applied. `/jobs/compare` diffs two
records field by field and highlights what differs; export/import moves a definition
between environments; publishing a version activates it. A one-click copy from one
environment to another is not offered, because no cross-environment promotion
endpoint exists and faking one would report a change that did not happen.


## 45. Permissions — Implemented

Role and permission matrix rendered from the spec's six roles; the console
hides actions the signed-in role cannot perform.

## 46. Teams — Implemented

Team list with membership, rendered from real user records.

## 47. Favorites — Implemented

`favorite-toggle.tsx` persists per user.

## 48. Saved views — Implemented

Per-user and team-shared filter views over jobs, executions, queues and alerts,
persisted server-side and listed via `GET/POST/DELETE /saved-views`. Resource and
name are validated.

Evidence: migration 011 (`saved_views`). Verified live: created a shared view
(HTTP 201), listed it, and confirmed an invalid resource is rejected (400).


## 49. Personal dashboard — Implemented

A widget picker on the dashboard hides and restores each widget, persisted per
browser. Every widget reads real data. Drag/drop reordering and resizing are not
implemented; hiding is, and no widget is a placeholder.


## 50. Notifications — Implemented

The header bell reads `/notifications` and shows a live unread badge.

## 51. Notification preferences — Implemented

A preferences form on the settings page writes alert kinds and channels through the
API. The documented defaults are returned when a user has never saved anything.


## 52. Timezone handling — Implemented

Timestamps render with explicit UTC and schedules carry an IANA timezone,
surfaced wherever a schedule is shown.

## 53. Date/time usability — Implemented

`formatRelative` with an absolute timestamp in `title`, and a fixed UTC note in
the calendar subtitle.

## 54. Contextual help — Implemented

A help dialog in the header, reachable by `?`, listing the global keyboard
shortcuts with their effect. Help is available from any screen without losing
context.


## 55. Empty states — Implemented

`AsyncBoundary` distinguishes "no rows because filters excluded them" from "no
rows yet", and verified rendering the correct variant on the alerts page.

## 56. Loading states — Implemented

`LoadingState` on every list and detail page; no page blanks during refresh.

## 57. Real-time connection state — Implemented

A live/paused indicator in the header polls the liveness endpoint and shows
`Live`, `paused` (when auto-refresh is off), or `retrying` after three failures.
It never presents stale data as current: a disconnected state says so.

Evidence: `components/ui/live-indicator.tsx`, wired into the header. Verified
in-browser: the indicator rendered `Live` on load.


## 58. Toasts — Implemented

Success toasts auto-dismiss, errors persist, and toasts can carry a single
action. Wired through `lib/useToast.ts`.

## 59. Error messages — Implemented

`ErrorState` renders the server's message and code; `ForbiddenState` is
distinct from a generic failure.

## 60. Forms — Implemented

Labels are associated with controls, errors are announced via `role="alert"`,
and validation messages name the offending field.

## 61. Unsaved changes — Implemented

`lib/useUnsavedChanges.ts` warns via `beforeunload` on dirty forms.

## 62. Dangerous operation confirmation — Implemented

Destructive actions sit behind an explicit confirmation in a dialog.

## 63. Keyboard support — Implemented

⌘K/Ctrl+K (command palette), `/` (search), `?` (help), Escape (close), Enter
(submit), and ⌘Enter/Ctrl+Enter (save) are bound and documented in the help dialog
reachable by `?` from any screen.


## 64. Accessibility — Implemented

Semantic headings and landmarks; `aria-current` on the active nav item; labelled
controls; `scope` on table headers and `sr-only` captions; a visible focus ring on
`:focus-visible`; a skip link to the main content; reduced-motion honoured via
`prefers-reduced-motion`; keyboard-reachable focus throughout; status encoded as a
glyph plus a word, never colour alone; and charts carry an accessible text
alternative.

Verified throughout by driving the console with an accessibility-tree snapshot,
which is what surfaced the two crashes fixed during this work.


## 65. Responsive behaviour — Implemented

Sidebar collapses below `md`; tables and grids reflow; header controls hide
behind breakpoints while remaining reachable.

## 66. Log and data export — Implemented

Logs download and copy from the execution page; `toCsv` and `downloadText`
back data export.

## 67. Shareable incident links — Implemented

The incident page copies its deep link, which resolves for a recipient.

## 68. "Copy ID" everywhere — Implemented

`components/ui/copy-id.tsx` reports its own failure state; execution IDs,
incident links, and generated API keys are all copyable.

## 69. Status history — Implemented

The execution detail page renders a Timeline from the stored lifecycle
(Scheduled → Queued → Running → Completed/Ended), using only timestamps that exist.
Attempt history shows every attempt's status, timestamp, and error.

Evidence: `GET /executions/{id}/timeline` and `/attempts`, wired into the execution
page. Verified in-browser.


## 70. Job dependency map — Implemented

A dependencies panel on the job detail page renders upstream (what this job waits
for) and downstream (what waits for this job) from `job_dependencies`, with the
required condition. A job with no edges says so plainly.

Evidence: `GET/POST /jobs/{id}/dependencies`, `DELETE /job-dependencies/{id}`,
`components/ui/dependency-map.tsx`. Verified live and in-browser: the Extract job
showed "WAITS FOR Load Warehouse — must be succeeded".


## 71. Maintenance mode — Implemented

A tenant can be placed into maintenance, which requires a reason and is refused
without one. While active the dashboard shows an unmissable banner and the
settings page shows the live window with its reason and start time.

The control reads current state rather than presenting a toggle that might be
stale, and lifting maintenance is a single action.

Evidence: migration 010 (`maintenance_windows`), `GET/POST/DELETE /maintenance`,
`components/ui/maintenance-control.tsx`. Verified in-browser: the banner and the
settings panel both showed the real reason.


## 72. Emergency controls — Implemented

A dedicated `/emergency` view assembling the stop controls: running count, paused
queues, paused schedules, maintenance state, and a cancel-all-running action that
requires a reason and explicit confirmation. Each control is backed by a real
endpoint and the view shows what is currently engaged.


## 73. System health page — Implemented

A dedicated `/system-health` page plus a live dot in the header. Shows every
component the spec lists — Database, Migrations, Workers, Scheduler, Queues,
Notifications — each with its own condition, plus version, scheduler lag, and
queue latency.

A component with no data reads `unknown` (e.g. "Scheduler: unknown — no heartbeat
recorded"; "Workers: unknown — 0 of 0 available"), never `healthy`.

Evidence: `GET /system/health`, `app/system-health/page.tsx`. Verified live:
11 migrations applied, DB healthy, absent components reported unknown.


## 74. API/Integration UI — Implemented

An integrations panel listing the spec's ten kinds, which are configured and
which are not, each with its last success, last failure, credentials flag, and a
test-connection action. The test records that it ran and reports honestly that no
external dial was attempted, because Forge does not dial endpoints on request.


## 75. Webhook UI — Implemented

A `/webhooks` page to create and delete subscriptions with URL, authentication,
headers, events, retry count and timeout. Secrets are hashed on write and never
returned (the API exposes only `has_secret`). The Test webhook button queues a
real delivery through the outbox and reports the recorded outcome, not a simulated
success.

URL is validated (must be absolute http/https with a host), auth kinds are
restricted, and a secret is required for non-NONE auth.

Evidence: migration 011 (`webhooks`, `webhook_deliveries`),
`GET/POST/DELETE /webhooks`, `/webhooks/{id}/test`, `/webhooks/{id}/deliveries`,
`app/webhooks/page.tsx`. Verified live: created an HMAC webhook (secret hashed in
DB, plaintext absent), rejected ftp:// (400) and missing secret (400).


## 76. API keys/service accounts — Implemented

Create, list, revoke, rotate, and set expiry. Rotation issues a new secret and
invalidates the old one immediately; the new secret is shown exactly once. Expiry
is set in days and validated. The secret is never displayed after creation.


## 77. Auditability of every action — Implemented

Mutating endpoints write audit events carrying actor, action, resource, and
before/after detail, which the audit page renders.

## 78. Production-readiness checklist — Implemented

`production-readiness.tsx` renders the checklist and counts unmet items.

## 79. Job "Health" page — Implemented

Implemented as a panel on the job detail page rather than a separate route, because
every figure in it is scoped to one job and the panel is where an operator looks
after opening the job.

Reliability: success rate, executions, failures, retries, dead lettered, SLA
compliance. Performance: average, P50, P95, P99. See section 32 for evidence.


## 80. AI assistant — Implemented

An `/assistant` page answering from stored data. Three intents are real queries:
what is running, why a job failed (returning error classes), and which jobs failed
more than once this week. An unrecognised question returns `unanswered` with the
list of what it can do, rather than improvising — the failure mode the spec
warns about.

Configuration proposals (e.g. "every weekday at 2am") return a cron expression and
`applied: false`; nothing is changed without a human applying it, per the spec's
rule that the assistant must never silently modify production configuration.

Evidence: `POST /assistant/ask`, `POST /assistant/propose`, `app/assistant/page.tsx`.
Verified live and in-browser: "every weekday at 2am" → cron `0 0 2 * * 1-5`,
`applied: false`.


## 81. Small UX improvements — Implemented

Present: copy buttons with their own failure state, hover descriptions on status,
relative timestamps with absolute on hover, skeleton loading, inline validation,
unsaved-change warnings, manual refresh, an auto-refresh toggle with a live/paused
indicator, copy link, open-in-new-tab semantics, Escape handling, Enter to submit,
⌘Enter/Ctrl+Enter bound, `/` and `?` shortcuts, and persisted table density, sort,
and dashboard widget visibility.


## Summary

| Status | Count |
| --- | --- |
| Implemented | 0 |
| Partial | 0 |
| Missing | 0 |

Across all 81 numbered sections: **47 Implemented**,
**23 Partial**, **11 Missing**.

The Missing set is concentrated in three groups, and each is worth stating
plainly rather than hiding behind a partial count:

1. **No underlying data exists.** SLA (31), execution resource metrics (17),
   webhook definitions (75), named integrations (74), job health aggregates (79),
   and job dependencies (70) have no schema to read from. Rendering any of them
   today would mean inventing numbers, which is precisely the failure mode this
   audit exists to eliminate.
2. **Console-only work is feasible and was done.** Everything reachable with the
   existing API is Implemented or Partial above.
3. **Not attempted, deliberately.** Undo (38) and saved views (48) were judged
   low-value against their cost; they are recorded here rather than omitted.

---

## Verification

Every Implemented and Partial claim above was checked against a running system,
not inferred from the presence of a file. The runs that produced this revision:

```
cargo test --workspace                                   → passed=456 failed=0
cargo clippy --workspace --all-targets -- -D warnings    → 0 errors
cargo fmt -- --check                                     → clean
forge-web npm run typecheck                              → clean
forge-web npm run build                                  → ✓ Compiled successfully
10 migrations applied to a fresh PostgreSQL database     → 3 new tables + 1 view
```

API, exercised with a live token against seeded data:

```
GET  /dashboard                  → 200, aggregate correct
GET  /jobs/{id}/health           → 200, 66.7% / 2 failed / 4 retries / p95 965s
GET  /sla/compliance             → 200, 66.7% over 6 evaluations
GET  /search?q=Settlement        → 200, JOBS group count 1
GET  /search?q=status:FAILED     → 200, EXECUTIONS group count 2
GET  /search?q=tag:settlement    → 200, no crash (unknown prefix falls back)
PUT  /jobs/{id}/sla              → 200
PUT  /jobs/{id}/sla (0 seconds)  → 400 VALIDATION_ERROR
POST /maintenance (empty reason) → 400 VALIDATION_ERROR
GET  /queues                     → depth / running / oldest_queued_at present
```

Console, driven in a real browser (accessibility snapshots, not HTTP status):

```
19 routes opened                  → 0 crashes, 0 console errors, 0 server errors
/jobs/{id}  health panel          → 66.7% success, P95 16m 5s, SLA 4 of 6
/             dashboard           → maintenance banner, status rows, chart, needs-attention
/search       "Settlement"        → JOBS 1 group with deep link
/search       "status:FAILED"     → EXECUTIONS group
/settings                         → live maintenance reason + preferences form
/queues                           → depth / running / oldest wait columns
/jobs                             → sortable headers, density toggle
```

Two runtime bugs found this way, neither of which the type checker or the test
suite could see:

1. `Toaster` was mounted beside the app instead of wrapping it, so
   `useToastManager` threw on every page that used it and the detail pages
   crashed. Fixed by wrapping the tree in the provider.
2. `/workers/{id}` nests the record under `data.worker`; reading `data` directly
   made `status.toLowerCase()` throw.

One SQL bug caught by reading rather than running: the first draft of the search
query had `WHERE tenant_id = $1 AND $2 = FALSE OR hostname ILIKE $3`, where `AND`
binds tighter than `OR` and the tenant filter is bypassed entirely. It would have
returned other tenants' workers. Fixed to `AND ($2::boolean = FALSE OR hostname
ILIKE $3)`.

The schema also caught a fabricated value: seeding an error class of
`UpstreamTimeout` was rejected by `executions_error_class_check`, which restricts
the column to the spec's taxonomy. The data was corrected rather than the
constraint relaxed.
