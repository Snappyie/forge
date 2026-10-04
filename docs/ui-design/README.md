# Forge console — UI design and screen flow

A proposed website and application UI for Forge, covering the full flow from
landing page to emergency stop. Every screen here is a rendered mockup, not a
description of one: `images/` holds the PNGs, and the source that generates them
lives beside this file.

## Why this document exists

`docs/UI.md` is an 81-section checklist written as a design brief. It is
ambitious and largely correct, and most of it is already built in `forge-web/`.
What it does not have is a single coherent *flow* — the order a person actually
moves through the product, and the reasoning for the order.

This document supplies that flow, and the mockups are the argument for it.

## How the mockups are built

The screens are plain HTML styled by one stylesheet, rendered with headless
Chrome at 2x. There is no framework and no build step.

```bash
cd docs/ui-design
node screens-brand.js && node screens-core.js
node screens-ops.js   && node screens-states.js
./render.sh                     # writes images/*.png
```

`forge.css` deliberately reuses the real token set from
`forge-web/src/app/globals.css` — the OKLCH grayscale palette, the 10px radius
scale, the `Inter` + `JetBrains Mono` pairing. The mockups are the same product
in the same clothes, so a design decision here is a decision you can implement
rather than re-litigate.

> One real bug surfaced while reading the existing console, unrelated to the
> mockups: `globals.css` sets `:focus-visible { outline: 2px solid
> hsl(var(--primary)) }` while every custom property is declared as a raw
> `oklch()` value. `hsl(oklch(…))` is invalid, so the focus ring does not render.
> Since keyboard focus is a hard requirement in `docs/UI.md` §64, the fix is to
> drop the `hsl()` wrapper and use `var(--primary)` directly.

## The register split

Forge has two interfaces and they should not look alike.

**Brand register** — the landing page, sign-in, and register. Here the interface
*is* the experience, so it gets type scale, motion, and a dark hero. The proof
object is not an abstract dashboard: it is a real execution timeline with its
four stages, next to the actual "why didn't Oct 2 run" verdict.

**Product register** — everything behind the login. The instrument, not the
experience. Dense, quiet, grayscale, with colour reserved exclusively for state.
An operator who opens these screens daily should move without thinking.

The current console has a third, accidental register: `/teams` and `/settings`
use oversized headings, radial gradients, and entrance animations while the rest
of the product is a dense operator console. Two of those pages are stubs and one
duplicates `/admin`. Pick one register and delete the other.

## The flow

### 1. Arrive — `images/01-landing.png`

The headline is a claim the product can actually keep: *"Every scheduled job
answers: why did it run, and why not?"* The hero carries the proof immediately
— a timeline and a diagnosis — because the credibility of a scheduler is
demonstrated with a schedule, not with a feature list.

### 2. Create a tenant — `images/02-register.png`

Registering creates a tenant and makes you its Owner. The right panel previews
the three steps to a first execution, so the form sets an expectation instead of
ending in an empty dashboard.

### 3. Sign in — `images/03-sign-in.png`

Deliberately shows a failure state rather than a pristine form: the error says
what was wrong, offers the token path as a fallback, and the aside shows live
production status so the operator knows what they are signing into.

### 4. First run — `images/14-onboarding.png`

This screen does not exist yet and is the largest real gap in the product. Today
first-run is seven manual steps with no in-product guidance, and
`docs/ui-parity-matrix.md` confirms the `/jobs` empty state has no "create your
first job" action despite `UI.md` §55 asking for one.

The checklist is ordered by dependency and honest about state: it separates
"already working" from "still needs you", and it ends at *connect a worker*,
because an execution with no worker just queues forever. That is the step most
likely to strand a new user, so it gets the code block and the copy button.

### 5. Monitor — `images/04-dashboard.png`

Answers the spec's three questions in order: is the scheduler healthy, what
needs attention, what is happening now.

The maintenance banner sits above everything because nothing is scheduling while
it is active. Metric cards link into the filtered list behind each number, so a
count is never a dead end. "Needs attention" leads because it is the reason an
operator opened the page. Component health reports `unknown` for notifications
rather than a misleading green — there is no evidence either way, and that is a
different fact from healthy.

### 6. Define — `images/05-jobs.png`, `images/06-job-builder.png`, `images/17-job-detail.png`

The jobs table is the workhorse. It carries next run *and* last run side by side,
because "when does it next fire" and "did it last fire" are the two questions
that decide whether you trust a schedule. Success rate gets a meter rather than a
bare percentage, so a 61% row is visible before the number is read.

The builder's schedule step is the most valuable control in the product and gets
the most space. Three things happen there:

- Natural language and raw cron are two doors into one value. The interpreted
  result is always shown before it can be applied — the assistant proposes, it
  never applies.
- The next-six-runs table is computed by the same engine that fires the job, in
  both local time and UTC, with DST conflicts called out.
- Misfire policy is asked as a plain question with consequences attached, not
  buried as a dropdown. "Catch up every missed run" says it is dangerous for a
  job that charges a card.

### 7. Explain — `images/07-why-didnt-run.png`

The signature screen, and the one worth the most engineering.

It evaluates every gate that could have suppressed an occurrence — schedule,
scheduler, cron match, calendar, maintenance, concurrency, dependency, worker —
and shows all of them, including the ones that passed. Showing the passes is what
makes the failure credible: a wrong guess is visibly wrong.

The reason is then stated in a sentence with the next eligible run attached, and
the resolved trigger chain (trigger → expression → timezone → next run) is shown
so the operator can see exactly what produced the time.

Calendar exclusion is called out explicitly here and on the calendar itself. Note
that Forge's `MisfirePolicy` already covers scheduler outages; a holiday or
business calendar is a different gate and is not yet in the domain model. That
gap is worth closing, because "the schedule matched and we were healthy but it
still did not run" is otherwise very hard to explain.

### 8. Operate — `images/08-executions.png`, `images/09-execution-detail.png`, `images/10-running-now.png`

Execution detail is the richest surface in any scheduler, and the mockup spends
its space accordingly: tabs for overview, logs, attempts, timeline, input/output,
metrics, and events; a gantt showing where the 4m 12s actually went; a timeline
that names the queue pause as the cause of the wait; attempt history with the real
backoff intervals; and a log viewer with line numbers, stream filtering, and a
search hit highlighted.

The failure banner leads with the diagnosis and the exhausted budget rather than
a raw status. Copy says "the retry budget is spent, so this execution will not
run again on its own" — an operator should never have to infer that.

Error classes render in full. `DEPENDENCY_UNAVAILABLE` truncated to
`DEPENDENCY_UNAVAILABL` hides the exact value you need, so long machine tokens
get a monospace pill that never clips.

### 9. Plan — `images/11-calendar.png`, `images/12-workflows.png`

The calendar answers "when is everything running" in one view, with a clearly
marked today and skipped occurrences shown as skipped rather than omitted. A
missing run is information.

The workflow designer shows an execution waiting on a human approval with the
wait surfaced as a warning, because a run blocked for 18 minutes is a problem
nobody is paged for.

### 10. Govern — `images/13-emergency.png`

Everything destructive requires a typed reason, and the screen states plainly what
survives: queued work stays queued, attempt history is kept, and a worker that
ignores cancellation loses its lease in 20 seconds and is reaped. An emergency
control that does not tell you its blast radius is not safe to use.

### Cross-cutting — `images/15-states.png`, `images/16-command-palette.png`

`AsyncBoundary` already enforces loading, empty, error, permission-denied, and
degraded structurally, which is the right call. The states screen documents all
six and adds the distinction that matters most in practice: *empty* (nothing here
yet, with the action that fills it) is not *filtered to nothing* (clear a filter).

The command palette searches across actions, jobs, and executions at once, and
the two things it must never do — apply a change, or guess — are stated as
product rules rather than left to implementation.

## Navigation

The current sidebar is a flat list of 20 items with no hierarchy, which is why
"Schedules", "Calendar", "Running now", and "Upcoming" all compete as
unlabelled clock icons. Grouping by work pattern teaches the mental model:

- **Monitor** — Dashboard, Running now, Upcoming, Alerts, Incidents
- **Define** — Jobs, Schedules, Workflows, Calendar
- **Capacity** — Queues, Workers
- **Govern** — Audit log, Integrations, Administration

Counts carry state colour, so a red badge next to Alerts is the same red as a
failed execution.

## What is implemented

The mockups were built first, then implemented in `forge-web/` and verified
against a live stack (Postgres, `forge-server`, real seeded data) rather than
only typechecked.

| Change | File |
|---|---|
| Grouped navigation with live counts | `src/components/layout/nav.ts`, `src/components/layout/AppShell.tsx` |
| First-run checklist | `src/components/ui/getting-started.tsx` |
| Log viewer: search, stream filter, line numbers | `src/components/ui/log-viewer.tsx` |
| Non-truncating error badge, status glyphs | `src/components/status-badge.tsx` |
| Failure headline and retry action | `src/app/executions/[id]/page.tsx` |
| Focus-ring token fix | `src/app/globals.css` |
| Session bootstrap fix | `src/lib/auth.tsx` |
| Next-runs preview for an unsaved expression | `src/components/ui/schedule-preview.tsx` |
| Schedule step rebuilt to match the mockup | `src/components/ui/job-wizard.tsx` |
| Stateless preview endpoint | `crates/forge-api/src/schedules.rs`, `router.rs` |

### The schedule step, and why it needed a backend change

The mockup's most valuable element on this screen is the next-runs preview, and
it could not be built as designed. The only preview endpoint,
`POST /schedules/{id}/preview`, requires a **saved** schedule — so it was useless
while authoring one, which is the only time a preview matters.

`POST /schedules/explain` evaluates an expression that does not exist yet. It
calls the same `forge_scheduler::preview_occurrences` the scheduler uses, so the
preview still cannot disagree with what fires (spec 09.13). It reads and writes
nothing, and it requires a timezone exactly as a saved schedule does: a preview
that silently fell back to the server's local time would disagree with the
scheduler on the one point the screen exists to establish.

The step itself now has pattern presets, a timezone picker, a plain-English
reading of the cron expression, the six-run preview in both local time and UTC,
and DST-conflict flags. The misfire radio group and the manual-only toggle are
wired to real draft state and sent on publish — previously they were decorative,
so the choice an operator made about a missed run was silently discarded.

### Bugs found and fixed while verifying

These were not part of the design work; they surfaced because the console was
run against a real server.

**The focus ring never rendered.** `globals.css` declared `:focus-visible` with
`hsl(var(--primary))` while every custom property holds a raw `oklch()` value.
`hsl(oklch(...))` is invalid, so the rule was dropped and keyboard focus was
invisible — a hard requirement in `docs/UI.md` §64.

**Signing out on every page load.** Forge refresh tokens are single-use: spending
one revokes it. The auth provider exchanged the stored token on mount, and the
API client was wired in an effect that ran *after* the first child effects had
already fired requests. Those early requests went out with no `Authorization`
header, came back 401, and tripped `onUnauthorized`, which cleared a perfectly
valid session. Fixed by publishing the token through a ref that `applySession`
updates synchronously, and by collapsing refresh exchanges so a repeated call
cannot spend an already-rotated token.

**A missing UI primitive.** `job-wizard.tsx` imported `@/components/ui/radio-group`,
which did not exist. It is now implemented against `@base-ui/react/radio-group`,
matching the conventions of the neighbouring primitives.


**The wizard overflowed its own shell.** Its root used
`h-[calc(100vh-3rem)]` to compensate for the application header, but it renders
*inside* that header's shell. Subtracting the height twice pushed the step rail
past the viewport and the shell's footer overlapped the wizard's own.

**The pattern picker displayed its id, not its label.** base-ui's `SelectValue`
falls back to the selected item's value, so the control read "weekday" rather
than "Every weekday at a specific time". It now renders the label explicitly.

Cron parse failures also read `invalid cron expression: Invalid expression:
Invalid cron expression.` — the same problem named three times, because `cron`
0.12 emits no detail at all. That boilerplate is now dropped in favour of what
Forge expected. All 69 `forge-scheduler` tests pass.

Three latent type errors were also fixed because they blocked the build:
`executions/page.tsx` compared `trigger_source` against `"SCHEDULED"` (a status,
not a trigger — the value is `SCHEDULE`), `jobs/[id]/page.tsx` read
`job.queue_id`, which does not exist on `Job` (it is `default_queue_id`), and
three `<Select>` handlers assigned base-ui's nullable value into a `string`
field.

## What I did not design

Honesty about scope, because these are the gaps that will be noticed:

- **Log search, regex, level filters, streaming.** `ui-parity-matrix.md` flags
  these as missing and they are the weakest Tier-1 feature. The mockup shows a
  search box and a filter chip, which is the shape of the fix, not the feature.
- **`/teams` and `/settings`.** `/teams` is a hardcoded stub calling an endpoint
  that does not exist. It should either get a schema and an API or come out of
  the navigation.
- **Approval endpoints.** The workflow mockup shows Approve and Reject, but no
  HTTP route exists for them. That is a backend gap before it is a UI gap.
- **Alerts and incidents detail.** The parity matrix lists 11 missing sections
  including Assign, Snooze, Resolve, and incident context.

## One structural recommendation

The two registers should be enforced, not left to whoever opens the next file.
`/teams` and `/settings` currently read as a marketing page inside an operator
console, which is the exact failure mode the register split exists to prevent.