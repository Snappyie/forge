# Deploying Forge

Forge is packaged as a Helm chart at `deploy/helm/forge`. It deploys the API
(which also runs the scheduler and the lease reaper), an optional Next.js
console, and a migration step that runs before any replica starts.

## Requirements

- Kubernetes 1.24 or newer
- Helm 3
- PostgreSQL 14 or newer, reachable from the cluster
- A pushed image. **The chart will not render without one.**

## The database user needs CREATEROLE

Migration `020_row_level_security.sql` creates a `forge_app` role so row-level
security has something to bind its policies to. Creating a role is cluster-level
state, so the database user Forge connects as must be permitted to do it:

```sql
ALTER ROLE forge CREATEROLE;
```

Without it the migration fails with
`while executing migration 20: permission denied to create role`. This is a real
requirement of the schema rather than a limitation of the chart, and it is the
only superuser-adjacent privilege Forge needs.

## Install

```bash
helm upgrade --install forge deploy/helm/forge \
  --namespace forge --create-namespace \
  --set image.repository=ghcr.io/your-org/forge \
  --set image.tag=0.2.0 \
  --set secrets.authSessionSecret="$(openssl rand -base64 48)" \
  --set secrets.apiKeyHashingSecret="$(openssl rand -base64 48)" \
  --set database.url="postgres://forge:...@postgres:5432/forge"
```

Or supply the secrets separately, which keeps them out of release values:

```bash
kubectl create secret generic forge-db \
  --from-literal=database-url="postgres://forge:...@postgres:5432/forge" -n forge
kubectl create secret generic forge-secrets \
  --from-literal=auth-session-secret="$(openssl rand -base64 48)" \
  --from-literal=api-key-hashing-secret="$(openssl rand -base64 48)" -n forge

helm upgrade --install forge deploy/helm/forge -n forge \
  --set image.repository=ghcr.io/your-org/forge --set image.tag=0.2.0 \
  --set existingSecret=forge-secrets \
  --set database.existingSecret=forge-db
```

## One binary, no Node

The console is statically exported and compiled into `forge-server`, so the
image contains one executable and no Node runtime, no `node_modules` and no
static file tree. The chart deploys a single workload; there is no separate
console Deployment to keep in step with the API.

That works because the console needs no server: every page is a client component
that fetches from the API on mount, and the API sits at `/api/v1` on the same
origin, so requests are same-origin and no CORS configuration is involved.

Two consequences worth knowing:

- **Rebuilding the console changes the binary.** `NEXT_PUBLIC_*` values are
  inlined at build time, so a console change is a new `forge-server` - not a new
  file copied into a running pod. A pod restart alone will not pick it up.
- **Deep links work.** `/jobs/<uuid>` boots the console and the client router
  resolves the segment. The export cannot enumerate UUIDs, so it emits one shell
  per dynamic route and the server falls back to it; the id is then read from the
  URL in the browser. Pasting or bookmarking a detail URL is safe.

## Building the image

```bash
docker build -t forge/server:0.3.0 .
```

The builder stage installs Node to produce the export; the runtime stage does not
have it. The first `cargo build` on a clean checkout also builds the export
automatically, so no separate step is needed locally.

## What the chart refuses to do

These fail the render with an explanation rather than producing a deployment that
looks fine and breaks later:

| Missing value | Why it matters |
| --- | --- |
| `image.tag` / `image.digest` | An unpullable image fails as `ErrImagePull`, which reads like a cluster problem. |
| `secrets.authSessionSecret` | `forge-config` treats an empty string as *present*, so a pod would start and fail inside the crypto. |
| `secrets.apiKeyHashingSecret` | Same. |
| `database.external: false` | The chart does not manage Postgres; point it at one you operate. |

## How migrations run

`forge-server --migrate-only` applies the schema and exits. The chart runs it as
a `pre-install,pre-upgrade` Helm hook, so a release cannot roll out against a
schema the new code does not expect, and a failed migration stops the upgrade
rather than leaving half the replicas running.

The server still migrates on a normal start. That is what `run_local.sh` and
docker-compose rely on. In Kubernetes the hook is what matters: three replicas
starting together would otherwise race each other on the same DDL.

The migration Secret and ServiceAccount are hooks too, weighted `-10` so they
exist before the migration Job runs.

## Scheduling and leader election

The scheduler runs inside every API replica. Leadership is a Postgres advisory
lock acquired per tick, so only one replica schedules at a time and the others
quietly stand by - extra replicas are safe. Set `scheduler.enabled=false` on a
deployment that should never schedule.

## Workers

Workers are not part of this chart. They are whatever your jobs are written in;
point them at the service and give them a worker token:

```bash
kubectl port-forward -n forge svc/forge 3000:3000
FORGE_TEST_API=http://localhost:3000/api/v1 ./scripts/verify-sdks.sh
```

A worker needs only three permissions - `workers:claim`, `workers:heartbeat` and
`executions:write`. Registering one needs an operator credential; see
`sdk/*/examples/e2e.*` for a worked example of the full handshake in each
language.

## Network policy

`networkPolicy.restrictEgress: true` allows only DNS and port 5432. **That breaks
`HTTP_REQUEST` jobs**, which call arbitrary hosts. For an environment where jobs
reach external services use `restrictEgress: false`, or list the specific hosts
under `extraEgress`.

The policy is off by default for that reason.

## Verifying a release

```bash
kubectl get pods -n forge -l app.kubernetes.io/component=api
kubectl logs -n forge -l app.kubernetes.io/component=migrate
kubectl port-forward -n forge svc/forge 3000:3000
curl -fsS localhost:3000/api/v1/health/ready
```

`/api/v1/health/live` is public and touches no dependency; `/api/v1/health/ready`
checks the database. Both are wired to the corresponding probes, so liveness
cannot be triggered by a database blip taking every pod out at once.