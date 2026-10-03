# 7. Web UI Specification

## 7.1 Technology Stack

The Web UI MUST be built using:
- **Framework:** Next.js (App Router recommended).
- **Styling:** Tailwind CSS.
- **Component Library:** shadcn/ui (providing accessible, customizable, unstyled components).

## 7.2 General UI rules

Every page MUST define:
- loading state
- empty state
- error state
- permission-denied state
- offline/degraded state where relevant
- keyboard navigation
- accessible labels
- confirmation for destructive actions

All timestamps MUST display timezone context.

## 7.3 Global shell

Header:
- Forge logo/name
- tenant selector
- global search
- notifications
- help/docs
- user menu

Sidebar:
- Dashboard
- Jobs
- Workflows
- Executions
- Queues
- Workers
- Audit
- Administration

## 7.4 Dashboard

Widgets:
- running executions
- queued executions
- failed executions
- success rate
- execution latency
- queue depth
- oldest queued job
- worker health
- recent incidents
- recent executions
- schedules due soon

Every metric card links to filtered detail.

## 7.5 Job list

Columns:
- name
- status
- queue
- latest execution
- success rate
- next run
- owner
- updated

Actions:
- trigger
- pause schedule
- open
- duplicate
- archive

Bulk actions MUST require confirmation.

## 7.6 Job editor

Sections:
1. Identity.
2. Execution.
3. Schedule.
4. Retry.
5. Timeout.
6. Concurrency.
7. Resources.
8. Environment.
9. Secrets references.
10. Notifications.
11. Advanced.

Provide live validation.

Show a human-readable summary:
“Runs every weekday at 02:00 Asia/Kolkata; maximum 2 concurrent executions; 5 retries with exponential backoff.”

## 7.7 Schedule preview

Show next 10/25/50 occurrences.

DST anomalies MUST be explicitly indicated.

Invalid schedules MUST provide actionable errors.

## 7.8 Workflow editor

Features:
- node palette
- canvas
- drag/drop
- connect nodes
- validation
- cycle detection
- zoom
- minimap
- node search
- dependency configuration
- per-node settings
- version history

The editor MUST prevent publishing invalid DAGs.

## 7.9 Execution list

Filters:
- status
- job
- workflow
- queue
- worker
- tenant
- time
- duration
- error code

Saved filters SHOULD be supported.

## 7.10 Execution detail

Tabs:
- Overview
- Timeline
- Logs
- Attempts
- Input
- Output
- Worker
- Resources
- Events
- Dependencies

Actions:
- cancel
- retry
- dead-letter
- rerun with modified input where authorized

## 7.11 Logs

Requirements:
- streaming
- search
- level filter
- time filter
- wrap toggle
- copy
- download
- correlation ID
- redaction

Secrets MUST never be displayed.

## 7.12 Worker page

Display:
- health
- version
- uptime
- capabilities
- CPU
- memory
- running jobs
- heartbeat age
- queue assignments
- drain state

Actions:
- drain
- revoke
- inspect

## 7.13 Queue page

Display:
- depth
- throughput
- oldest item
- priority distribution
- active workers
- concurrency limit
- recent failures

## 7.14 Audit page

Search and filter audit events.

Display:
- timestamp
- actor
- action
- resource
- result
- request ID

Provide event detail without secret material.

## 7.15 Administration

Sections:
- users
- roles
- API keys
- tenants
- queues
- retention
- system configuration
- integrations
- security

## 7.16 UX quality requirements

- No destructive action should be one click without confirmation.
- Long-running actions show progress or state transition.
- Stale data is indicated when appropriate.
- Tables support keyboard navigation.
- Copyable IDs have a copy button.
- Relative timestamps have exact timestamp tooltips.
- Empty pages explain how to create the first resource.
- Errors provide recovery actions where known.
