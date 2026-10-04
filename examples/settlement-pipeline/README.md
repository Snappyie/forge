# Forge integration example: settlement pipeline

A runnable tour of every Forge feature. It uses only the Python standard
library, so there is nothing to install.

```
python3 demo.py --base-url http://localhost:3000/api/v1 \
                --email you@example.com --password 'your-password'
```

The script prints what it did and what came back, so its output doubles as a
readable transcript of the API.

## What it exercises

| Step | What it shows |
| --- | --- |
| 1 | Registration is invitation-only after the first tenant; session refresh rotates the token |
| 2 | Jobs are drafts until a version is published; concurrency policy is attached to the version |
| 3 | Cron schedules, with a preview of the next occurrences and any DST anomalies |
| 4 | Triggering executions, then reading status, lifecycle timeline, logs, and metrics |
| 5 | Job health: success rate, retries, and duration percentiles |
| 6 | Setting an SLA target that compliance is later measured against |
| 7 | Alert rules, which only fire once a run actually fails |
| 8 | Webhooks, including the SSRF guard refusing an internal address |
| 9 | Building and publishing a workflow graph |
| 10 | Bulk operations and undoing one |
| 11 | Health, dashboard, and upcoming aggregates |
| 12 | Search and the assistant, which proposes but never applies |
| 13 | Maintenance mode, and lifting it |
| 14 | Queues: creating, pausing, and resuming |
| 15 | The worker protocol: registration, draining, heartbeat |
| 16 | Saved views |
| 17 | Integrations, and an honest connection test |
| 18 | API keys: create, rotate, revoke |
| 19 | Users and the audit trail |
| 20 | Notifications, preferences, and incidents |
| 21 | Emergency controls, and reading the OpenAPI contract |
| 22 | Cursor pagination |

## Running it

You need a reachable Forge server. To start one locally:

```
docker compose up -d postgres
cargo run -p forge-server
```

Then register once. The **first** registration claims the tenant as owner;
every later one needs an invitation token:

```
# first account, on an empty database
curl -X POST http://localhost:3000/api/v1/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"you@example.com","password":"correct-horse-battery"}'

# later accounts need an invite created by an owner
```

Pass `--invite <uuid>` to the demo when registering a new account. If you omit it
on a tenant that is already claimed, the demo tells you the exact SQL to create
the invitation rather than failing opaquely.

Every endpoint group in the API has a step above. The tour is deliberately
idempotent: each run uses a fresh suffix on job keys, so re-running it never
collides with the last one.

## Layout

- `forge_demo/client.py` — a dependency-free API client. Read this first; every
  call the demo makes is a method here.
- `demo.py` — the tour itself, one method per step.
