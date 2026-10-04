# Forge Architecture & Complete End-to-End Execution Flows

This document provides a comprehensive technical breakdown of how the **Forge** job and workflow orchestration platform works. It details the system architecture, component boundaries, background daemon loops, and complete sequence diagrams for every runtime flow.

---

## 1. System Architecture & Component Topology

Forge is engineered with a **Hexagonal (Ports & Adapters) architecture** in Rust. The domain core is decoupled from transport protocols, external databases, and environment variables.

### 1.1 High-Level Component Topology

```mermaid
flowchart TD
    subgraph Clients["Clients & Presentation"]
        UI["Web UI (Next.js 16 / TypeScript)<br/>30 Production Routes"]
        SDK_PY["Python Worker SDK"]
        SDK_TS["Node.js Worker SDK"]
        SDK_GO["Go Worker SDK"]
        SDK_JA["Java Worker SDK"]
        CLI["Forge CLI Client"]
    end

    subgraph Server["Forge Server (forge-server / Axum)"]
        subgraph APILayer["API & Security Layer (forge-api / forge-auth)"]
            Router["Axum HTTP Router & OpenAPI 3.1"]
            Auth["JWT & HMAC API Key Verifier"]
            WorkerAuth["Scoped Worker Token Verifier"]
            SSRF["SSRF Guard (RFC 1918 / Cloud Metadata Filter)"]
            RateLimit["Sliding Window Rate Limiter"]
        end

        subgraph CoreEngines["Core Domain & Orchestration Engines"]
            Scheduler["Scheduler Engine (forge-scheduler)"]
            WFDriver["Workflow Driver (forge-executor)"]
            Reaper["Lease Reaper & Recovery (forge-executor)"]
            Gate["Completion Gate (forge-executor)"]
            OutboxPub["Outbox Publisher (forge-events)"]
            RetPurge["Retention Cleaner (forge-server)"]
            Obs["Metrics & Tracing (forge-observability)"]
        end

        subgraph StorageLayer["Persistence Layer (forge-storage / SQLx)"]
            JobRepo["Job & Version Repository"]
            ExecRepo["Execution Repository"]
            SchedRepo["Schedule Repository"]
            WFRepo["Workflow Repository"]
            LeaseRepo["Lease Repository"]
            WorkerRepo["Worker Repository"]
            QueueRepo["Queue Repository"]
            OutboxRepo["Outbox Repository"]
            AuditRepo["Audit Repository"]
        end
    end

    subgraph Database["Primary Data Store"]
        PG[("PostgreSQL 15+<br/>• ACID Transactions<br/>• FOR UPDATE SKIP LOCKED<br/>• GIN JSONB Indexes<br/>• Row-level Multi-Tenancy")]
    end

    subgraph Outbound["External Sinks"]
        Webhooks["Customer Webhook Endpoints"]
        Alerts["Incident & Alert Channels"]
        Prometheus["Prometheus / OTel Collectors"]
    end

    UI --> Router
    CLI --> Router
    SDK_PY --> Router
    SDK_TS --> Router
    SDK_GO --> Router
    SDK_JA --> Router

    Router --> Auth
    Router --> WorkerAuth
    Auth --> CoreEngines
    WorkerAuth --> CoreEngines
    SSRF --> Webhooks

    Scheduler --> SchedRepo
    Scheduler --> ExecRepo
    WFDriver --> WFRepo
    WFDriver --> ExecRepo
    Reaper --> LeaseRepo
    Reaper --> ExecRepo
    Gate --> LeaseRepo
    OutboxPub --> OutboxRepo
    RetPurge --> ExecRepo
    RetPurge --> AuditRepo

    JobRepo --> PG
    ExecRepo --> PG
    SchedRepo --> PG
    WFRepo --> PG
    LeaseRepo --> PG
    WorkerRepo --> PG
    QueueRepo --> PG
    OutboxRepo --> PG
    AuditRepo --> PG

    OutboxPub --> Webhooks
    Obs --> Prometheus
    CoreEngines --> Alerts
```

---

### 1.2 Rust Workspace Crate Responsibilities

The codebase is structured into modular crates enforcing unidirectional dependencies:

| Crate | Purpose | Key Dependencies | Invariants |
|---|---|---|---|
| **`forge-domain`** | Pure business entities, state machines, validation rules, policy calculations. | None (zero I/O, zero network, zero SQL). | Contains no SQL, HTTP, or async primitives. 100% deterministic. |
| **`forge-storage`** | PostgreSQL persistence, schema migrations (001–017), repositories, transactional queries using `FOR UPDATE SKIP LOCKED`. | `sqlx`, `forge-domain`, `tokio` | Enforces multi-tenant isolation (`tenant_id`) on all queries. |
| **`forge-scheduler`** | CRON, interval, one-time calculation, IANA timezone resolution, DST gap/fold handling, misfire policies. | `chrono`, `chrono-tz`, `cron`, `forge-domain` | Independent of HTTP transport. Evaluates due occurrences algorithmically. |
| **`forge-executor`** | Execution lifecycle, worker protocol, lease acquisition/renewal, lease reaper, retry backoff, workflow execution graph. | `forge-domain`, `forge-storage` | Authoritative for task states, timeout sweeps, and completion gates. |
| **`forge-events`** | Transactional outbox publisher, domain event definitions, webhook dispatching with SSRF protection. | `reqwest`, `forge-domain`, `forge-storage` | Guarantees at-least-once delivery with exponential backoff. |
| **`forge-auth`** | Password hashing (Argon2id), JWT issuance/verification, worker tokens, HMAC-SHA256 API key hashing, RBAC permissions. | `jsonwebtoken`, `argon2`, `sha2`, `hmac` | Least-privilege enforcement. Worker tokens cannot become tenant admins. |
| **`forge-observability`**| Prometheus metrics registry, structured tracing subscriber, latency tracking. | `tracing`, `metrics`, `prometheus` | Low-overhead instrumentation across all critical execution paths. |
| **`forge-api`** | Axum REST router, OpenAPI 3.1 specification, request extractors, response envelopes, error mapping. | `axum`, all internal crates | Strict request validation; maps errors to standard JSON API envelopes. |
| **`forge-server`** | Application binary; coordinates server startup, database migrations, signal handling, and 6 background daemon loops. | `tokio`, `clap`, all internal crates | Graceful shutdown; failure of any background task triggers clean process termination. |

---

## 2. Background Daemon Loops (Runtime Composition)

Alongside the Axum HTTP API, `forge-server` executes six dedicated background loops:

```mermaid
flowchart LR
    subgraph ServerRuntime["forge-server Runtime"]
        L1["1. Scheduler Engine<br/>(Tick: 1000ms)<br/>Evaluates due schedules,<br/>creates executions, advances next_run_at"]
        L2["2. Lease Reaper & Sweep<br/>(Tick: 5000ms)<br/>Reclaims expired worker leases,<br/>re-queues retries, enforces timeouts"]
        L3["3. Workflow Driver<br/>(Tick: 1000ms)<br/>Advances DAG nodes,<br/>evaluates delays, branches & approvals"]
        L4["4. Outbox Publisher<br/>(Tick: 1000ms)<br/>Claims pending outbox events,<br/>delivers webhooks with HMAC signatures"]
        L5["5. Retention Cleaner<br/>(Tick: 3600s)<br/>Purges expired execution_logs<br/>and audit_events per tenant policy"]
        L6["6. Worker Heartbeat Monitor<br/>(Tick: 10s)<br/>Marks silent workers OFFLINE<br/>if heartbeat exceeds 60s window"]
    end
```

---

## 3. Detailed Sequence Diagrams of Complete System Flows

### Flow 1: Job Authoring, Versioning & Immutability

Jobs in Forge are immutable across versions. Publishing a version guarantees that past executions remain reproducible even if the job template changes.

```mermaid
sequenceDiagram
    autonumber
    actor Admin as Tenant Operator / UI
    participant API as forge-api (Jobs Handler)
    participant Storage as forge-storage (PostgreSQL)

    Admin->>API: POST /api/v1/jobs (name, key, description)
    Note over API: Verifies "jobs:write" permission
    API->>Storage: INSERT INTO jobs (status = 'DRAFT')
    Storage-->>API: Job created (ID: job_123, status: DRAFT)
    API-->>Admin: 201 Created (ApiResponse)

    Admin->>API: POST /api/v1/jobs/{job_id}/trigger
    API-->>Admin: 409 Conflict ("job has no published version")

    Admin->>API: POST /api/v1/jobs/{job_id}/versions
    Note over API: Payload: execution_type, timeout_seconds,<br/>retry_policy, concurrency_policy, worker_capabilities
    API->>Storage: INSERT INTO job_versions (version_number = 1, status = 'DRAFT')
    Storage-->>API: Version created (ID: ver_001)
    API-->>Admin: 201 Created

    Admin->>API: POST /api/v1/jobs/{job_id}/versions/{version_id}/publish
    API->>Storage: BEGIN TRANSACTION
    API->>Storage: UPDATE job_versions SET published_at = NOW() WHERE id = ver_001
    API->>Storage: UPDATE jobs SET status = 'ACTIVE', current_version_id = ver_001 WHERE id = job_123
    API->>Storage: COMMIT
    API-->>Admin: 200 OK (Job is now runnable)
```

---

### Flow 2: Worker Registration, Authentication & Heartbeat Protocol

Workers possess scoped, least-privilege tokens. A worker cannot access tenant administrative data, mint users, or alter schedules.

```mermaid
sequenceDiagram
    autonumber
    actor Worker as Worker Daemon (Python/Node/Go/Java)
    participant API as forge-api (Worker Gateway)
    participant Auth as forge-auth
    participant Storage as forge-storage (PostgreSQL)

    Note over Worker: Worker starts up on node 'host-01'
    Worker->>API: POST /api/v1/workers/register<br/>{ hostname: "host-01", capabilities: ["gpu", "compute"] }<br/>[Header: Authorization: Bearer <Admin/Provisioning Token>]
    API->>Auth: generate_worker_token()
    Auth-->>API: Raw token ("wkr_live_...") & SHA-256 Hash
    API->>Storage: INSERT INTO workers (id, hostname, capabilities, token_hash, status = 'READY')
    Storage-->>API: Worker Row persisted
    API-->>Worker: 201 Created { id: "w_888", token: "wkr_live_..." }

    Note over Worker: Worker stores token securely in memory

    loop Every 30 Seconds (Worker Liveness)
        Worker->>API: POST /api/v1/workers/w_888/heartbeat<br/>[Header: Authorization: Bearer wkr_live_...]
        API->>Auth: Verify worker token hash & bind to worker ID
        API->>Storage: UPDATE workers SET last_heartbeat_at = NOW(), status = 'READY' WHERE id = 'w_888'
        API-->>Worker: 200 OK
    end
```

---

### Flow 3: Scheduler Engine & Occurrence Evaluation Loop

The scheduler daemon runs concurrently across multiple server replicas, safely coordinating via PostgreSQL locks without external lock managers.

```mermaid
sequenceDiagram
    autonumber
    participant Loop as Scheduler Daemon Loop
    participant Engine as SchedulerEngine
    participant Cal as Recurrence Calculator (Cron/Interval/OneTime)
    participant Storage as forge-storage (PostgreSQL)
    participant Outbox as outbox_events

    loop Every 1000ms
        Loop->>Engine: tick()
        Engine->>Storage: SELECT id FROM schedules WHERE next_run_at <= NOW() AND status = 'ACTIVE'<br/>ORDER BY next_run_at ASC LIMIT batch_size FOR UPDATE SKIP LOCKED
        Storage-->>Engine: Locked due schedule rows

        loop For Each Claimed Schedule
            Engine->>Cal: compute next occurrence after current next_run_at
            Cal-->>Engine: next_run_at (adjusted for timezone, DST & misfire policy)

            Engine->>Storage: BEGIN TRANSACTION
            Engine->>Storage: Resolve target job_version_id (PINNED version or LATEST active)
            Engine->>Storage: INSERT INTO executions (id, job_id, job_version_id, queue_id, priority,<br/>status = 'QUEUED', scheduled_for, enqueued_at = NOW(), trigger_source = 'SCHEDULE')
            Note over Storage: Unique index (schedule_id, scheduled_for) prevents duplicate firings
            Engine->>Storage: UPDATE schedules SET next_run_at = new_next, last_run_at = NOW() WHERE id = sched_id
            Engine->>Outbox: INSERT INTO outbox_events (event_type = 'execution.queued')
            Engine->>Storage: COMMIT
        end
    end
```

---

### Flow 4: Job Execution Lifecycle (Queueing, Claiming, Leasing & Completion)

This flow details how a worker polls for work, claims a lease, sends periodic execution heartbeats, streams logs, and reports successful completion.

```mermaid
sequenceDiagram
    autonumber
    actor Worker as Worker SDK (e.g. Python / Go)
    participant API as forge-api
    participant Gate as CompletionGate
    participant Storage as forge-storage (PostgreSQL)

    Worker->>API: POST /api/v1/workers/w_888/claim<br/>[Header: Authorization: Bearer wkr_live_...]
    API->>Storage: BEGIN TRANSACTION
    Note over Storage: Query uses priority weighting + age term:<br/>ORDER BY (priority_score + age) DESC<br/>AND w.capabilities @> jv.worker_capabilities<br/>AND active_queue_executions < q.max_concurrency<br/>FOR UPDATE OF e SKIP LOCKED LIMIT 1
    Storage->>Storage: UPDATE executions SET status = 'DISPATCHED', worker_id = 'w_888', enqueued_at = NOW()
    Storage->>Storage: INSERT INTO leases (id, execution_id, worker_id, expires_at = NOW() + 20s)
    Storage->>Storage: COMMIT
    API-->>Worker: 200 OK { execution: { id: "exec_555", input: {...} }, lease: { id: "lease_999", expires_at: "..." } }

    Note over Worker: Worker begins executing task payload

    opt Long-Running Task: Execution Lease Renewal
        loop Every 10 Seconds
            Worker->>API: POST /api/v1/executions/exec_555/heartbeat<br/>{ lease_id: "lease_999", worker_id: "w_888" }
            API->>Storage: UPDATE leases SET expires_at = NOW() + 30s WHERE id = 'lease_999' AND worker_id = 'w_888'
            API-->>Worker: 200 OK { renewed: true }
        end
    end

    opt Live Output Streaming
        Worker->>API: POST /api/v1/executions/exec_555/logs<br/>{ lines: [{ stream: "stdout", content: "Processing chunk 1..." }] }
        API->>Storage: INSERT INTO execution_logs (execution_id, stream, content, logged_at)
        API-->>Worker: 201 Created
    end

    Note over Worker: Task finishes successfully with result data
    Worker->>API: POST /api/v1/executions/exec_555/complete<br/>{ lease_id: "lease_999", worker_id: "w_888", succeeded: true, output: { "processed": 100 } }
    API->>Gate: check(exec_555, lease_999, w_888)
    Note over Gate: Confirms lease was not reclaimed by LeaseReaper
    Gate-->>API: Verification Passed
    API->>Storage: BEGIN TRANSACTION
    API->>Storage: UPDATE executions SET status = 'SUCCEEDED', finished_at = NOW(), output = '{...}' WHERE id = 'exec_555'
    API->>Storage: UPDATE leases SET released_at = NOW() WHERE id = 'lease_999'
    API->>Storage: INSERT INTO execution_attempts (execution_id, attempt_number = 1, status = 'SUCCEEDED', exit_code = 0)
    API->>Storage: COMMIT
    API-->>Worker: 200 OK { status: "SUCCEEDED" }
```

---

### Flow 5: Execution Failure, Exponential Backoff & Dead-Letter (DLQ)

When an execution fails, Forge consults the job version's `retry_policy`. If retry budget remains, it reschedules with exponential backoff and jitter. If retries are exhausted, it transitions to `DEAD_LETTERED`.

```mermaid
sequenceDiagram
    autonumber
    actor Worker as Worker SDK
    participant API as forge-api
    participant FailHandler as FailureHandler (forge-executor)
    participant Storage as forge-storage

    Worker->>API: POST /api/v1/executions/exec_555/fail<br/>{ lease_id: "lease_999", error_message: "Connection timeout to DB", exit_code: 1 }
    API->>FailHandler: record_failure(exec_555, error, attempts_made = 1)
    FailHandler->>Storage: SELECT retry_policy FROM job_versions WHERE id = exec.version_id

    alt Attempts (1) < Max Retries (3)
        Note over FailHandler: Calculate Backoff Delay:<br/>delay = min(initial * backoff_factor^(attempt-1), max_delay) + jitter
        FailHandler->>Storage: UPDATE executions SET status = 'RETRY_SCHEDULED', retry_at = NOW() + delay, attempt_count = 1
        FailHandler->>Storage: INSERT INTO execution_attempts (execution_id, attempt_number = 1, status = 'FAILED', error = '...')
        API-->>Worker: 200 OK { status: "RETRY_SCHEDULED", retry_at: "..." }

        Note over Storage: Lease Reaper sweeps due retries once delay elapses:
        Storage->>Storage: UPDATE executions SET status = 'QUEUED' WHERE status = 'RETRY_SCHEDULED' AND retry_at <= NOW()
    else Retries Exhausted (Attempts >= Max Retries)
        FailHandler->>Storage: UPDATE executions SET status = 'DEAD_LETTERED', finished_at = NOW()
        FailHandler->>Storage: INSERT INTO execution_attempts (execution_id, attempt_number = max, status = 'DEAD_LETTERED')
        API-->>Worker: 200 OK { status: "DEAD_LETTERED" }
    end
```

---

### Flow 6: Worker Crash, Lease Expiration & Stale Completion Protection

If a worker node crashes or loses network connectivity mid-task, Forge's `LeaseReaper` automatically recovers the orphaned execution without human intervention.

```mermaid
sequenceDiagram
    autonumber
    participant Worker as Worker Node (Crashes)
    participant Reaper as LeaseReaper (Daemon Loop)
    participant Gate as CompletionGate
    participant Storage as forge-storage (PostgreSQL)
    participant NewWorker as Healthy Worker Node

    Note over Worker: Worker claims exec_777 with lease_111 (expires in 20s)
    Note over Worker: Worker process crashes (power failure / OOM kill)
    Note over Worker: No heartbeats sent; lease expires

    loop Every 5 Seconds
        Reaper->>Storage: SELECT * FROM leases WHERE expires_at < NOW() AND released_at IS NULL
        Storage-->>Reaper: Expired lease row (lease_111, exec_777)
        Reaper->>Storage: BEGIN TRANSACTION
        Reaper->>Storage: UPDATE leases SET released_at = NOW() WHERE id = 'lease_111'
        Reaper->>Storage: UPDATE executions SET status = 'QUEUED', worker_id = NULL WHERE id = 'exec_777' AND status IN ('DISPATCHED', 'RUNNING')
        Reaper->>Storage: COMMIT
    end

    Note over NewWorker: Healthy worker claims newly requeued task
    NewWorker->>Storage: Claim exec_777 -> receives lease_222 (active)

    opt Zombie Worker Resurrects Late
        Worker->>Gate: POST /executions/exec_777/complete with lease_111
        Gate->>Storage: SELECT id FROM leases WHERE id = 'lease_111' AND released_at IS NULL
        Storage-->>Gate: None (lease was released/reclaimed)
        Gate-->>Worker: 409 Conflict ("lease expired; recovered by another worker")
    end
```

---

### Flow 7: Workflow / DAG Orchestration Engine

Forge DAGs support linear chains, fan-out/fan-in concurrency, conditional decision branches, delay timers, and manual human approvals.

```mermaid
sequenceDiagram
    autonumber
    actor User as User / Scheduler
    participant API as forge-api (Workflows)
    participant Driver as WorkflowDriver (Daemon Loop)
    participant Storage as forge-storage (PostgreSQL)
    participant Worker as Worker Fleet

    User->>API: POST /api/v1/workflows/{id}/trigger { input: { "order_id": 999 } }
    API->>Storage: INSERT INTO workflow_executions (status = 'RUNNING', context = '{...}')
    API->>Storage: INSERT root node executions (status = 'PENDING')
    API-->>User: 202 Accepted { workflow_execution_id: "wf_run_1" }

    loop Workflow Driver Progression Tick (Every 1000ms)
        Driver->>Storage: Find workflow executions with status = 'RUNNING'

        alt Job Node (Ready for Dispatch)
            Driver->>Storage: INSERT INTO executions (status = 'QUEUED', workflow_execution_id = 'wf_run_1')
            Driver->>Storage: UPDATE workflow_node_executions SET status = 'RUNNING'
            Worker->>Storage: Worker claims, executes, and completes job node with output { "authorized": true }
            Storage->>Storage: UPDATE workflow_node_executions SET status = 'SUCCEEDED', output = '{...}'
        else Condition / Branch Node
            Driver->>Driver: Evaluate condition expression against parent output ("authorized == true")
            Driver->>Storage: Mark winning branch node PENDING; prune non-matching branch node as SKIPPED
        else Delay Node
            Note over Driver: Evaluates duration_seconds against node start time
            Driver->>Storage: If deadline passed, transition Delay node from RUNNING to SUCCEEDED
        else Manual Approval Node
            Driver->>Storage: Mark node 'WAITING_APPROVAL'
            Note over User: Operator reviews task in Web UI
            User->>API: POST /workflows/executions/wf_run_1/nodes/node_approve/approve
            API->>Storage: UPDATE workflow_node_executions SET status = 'APPROVED'
        end

        Driver->>Storage: Check if all terminal nodes succeeded
        Storage->>Storage: UPDATE workflow_executions SET status = 'SUCCEEDED', finished_at = NOW()
    end
```

---

### Flow 8: Transactional Outbox & Webhook Delivery

To prevent dual-write bugs, all domain events are written to the `outbox_events` table inside the same PostgreSQL transaction that modifies business state. The `OutboxPublisher` processes them asynchronously.

```mermaid
sequenceDiagram
    autonumber
    participant AppService as Storage Transaction
    participant OutboxTable as outbox_events Table
    participant Publisher as OutboxPublisher (Daemon)
    participant SSRFGuard as SSRF Protection Engine
    participant CustomerServer as Destination Webhook Endpoint

    AppService->>OutboxTable: INSERT INTO outbox_events (event_type, payload, status = 'PENDING')<br/>[Within same DB Transaction as Execution State Change]

    loop Every 1000ms
        Publisher->>OutboxTable: SELECT * FROM outbox_events WHERE published_at IS NULL<br/>ORDER BY created_at ASC LIMIT 50 FOR UPDATE SKIP LOCKED
        OutboxTable-->>Publisher: Batch of pending events

        loop For Each Event
            Publisher->>SSRFGuard: Validate webhook destination URL
            alt Destination is Local / Private RFC 1918 / Cloud Metadata (169.254.169.254)
                SSRFGuard-->>Publisher: Refuse (Security Violation)
                Publisher->>OutboxTable: UPDATE outbox_events SET error = 'SSRF Blocked', published_at = NOW()
            else Destination is Valid Public Webhook
                Publisher->>CustomerServer: HTTP POST /webhook<br/>[Headers: X-Forge-Event, X-Forge-Signature: HMAC-SHA256(payload, secret)]
                CustomerServer-->>Publisher: 200 OK
                Publisher->>OutboxTable: UPDATE outbox_events SET published_at = NOW(), attempts = attempts + 1
            end
        end
    end
```

---

### Flow 9: Active Data Retention & Background Cleanup

To keep database tables lean and ensure predictable index lookup latency, Forge continuously purges expired execution logs and audit entries based on configurable tenant retention windows.

```mermaid
sequenceDiagram
    autonumber
    participant Cleaner as RetentionCleaner (Background Task)
    participant Storage as forge-storage (PostgreSQL)

    loop Every 1 Hour (Configurable: retention.interval)
        Cleaner->>Storage: BEGIN TRANSACTION
        Note over Cleaner: Compute cutoff timestamps per tenant configuration:<br/>logs_cutoff = NOW() - retention.logs_days (e.g. 30 days)<br/>audit_cutoff = NOW() - retention.audit_days (e.g. 90 days)

        Cleaner->>Storage: DELETE FROM execution_logs<br/>WHERE logged_at < logs_cutoff<br/>LIMIT batch_size (e.g. 5,000 rows)
        Storage-->>Cleaner: Deleted N log rows

        Cleaner->>Storage: DELETE FROM audit_events<br/>WHERE created_at < audit_cutoff<br/>LIMIT batch_size (e.g. 5,000 rows)
        Storage-->>Cleaner: Deleted M audit rows

        Cleaner->>Storage: COMMIT
        Note over Cleaner: Emits structured log with total pruned row count
    end
```

---

## 4. Entity-Relationship Data Model

The PostgreSQL schema enforces strict foreign key referential integrity and multi-tenant isolation across all entities:

```mermaid
erDiagram
    TENANTS ||--o{ USERS : contains
    TENANTS ||--o{ JOBS : owns
    TENANTS ||--o{ QUEUES : defines
    TENANTS ||--o{ WORKERS : registers
    TENANTS ||--o{ SCHEDULES : configures
    TENANTS ||--o{ WORKFLOWS : designs

    JOBS ||--o{ JOB_VERSIONS : versioned_as
    JOBS ||--o{ EXECUTIONS : instantiates
    JOB_VERSIONS ||--o{ EXECUTIONS : runs_version

    QUEUES ||--o{ EXECUTIONS : buffers
    WORKERS ||--o{ LEASES : holds
    EXECUTIONS ||--o{ LEASES : locked_by
    EXECUTIONS ||--o{ EXECUTION_ATTEMPTS : tracks_attempts
    EXECUTIONS ||--o{ EXECUTION_LOGS : records_logs

    WORKFLOWS ||--o{ WORKFLOW_VERSIONS : versioned_as
    WORKFLOW_VERSIONS ||--o{ WORKFLOW_NODES : contains_nodes
    WORKFLOW_VERSIONS ||--o{ WORKFLOW_EDGES : connects_edges
    WORKFLOW_VERSIONS ||--o{ WORKFLOW_EXECUTIONS : instantiates
    WORKFLOW_EXECUTIONS ||--o{ WORKFLOW_NODE_EXECUTIONS : executes_node

    TENANTS {
        uuid id PK
        string name
        timestamp created_at
    }

    JOBS {
        uuid id PK
        uuid tenant_id FK
        string name
        string key
        string status
        uuid current_version_id
    }

    JOB_VERSIONS {
        uuid id PK
        uuid job_id FK
        int version_number
        jsonb retry_policy
        jsonb concurrency_policy
        jsonb resource_requirements
        int timeout_seconds
        timestamp published_at
    }

    EXECUTIONS {
        uuid id PK
        uuid tenant_id FK
        uuid job_id FK
        uuid job_version_id FK
        uuid queue_id FK
        uuid worker_id FK
        string status
        string priority
        jsonb input
        jsonb output
        timestamp deadline_at
        timestamp scheduled_for
        timestamp enqueued_at
    }

    LEASES {
        uuid id PK
        uuid execution_id FK
        uuid worker_id FK
        timestamp expires_at
        timestamp released_at
    }

    WORKERS {
        uuid id PK
        uuid tenant_id FK
        string hostname
        string status
        jsonb capabilities
        string token_hash
        timestamp last_heartbeat_at
    }

    QUEUES {
        uuid id PK
        uuid tenant_id FK
        string name
        boolean paused
        int max_concurrency
    }

    SCHEDULES {
        uuid id PK
        uuid tenant_id FK
        uuid job_id FK
        string recurrence_type
        string cron_expression
        bigint interval_seconds
        timestamp one_time_instant
        string timezone
        string misfire_policy
        timestamp next_run_at
    }
```

---

## 5. Summary of Key Invariants

1. **Zero Dual-Write Inconsistency**: State transitions and event outbox records occur within the same PostgreSQL ACID transaction.
2. **Zero Task Starvation**: Dispatch ordering computes `(priority_weight + age_in_seconds)` to ensure long-waiting lower-priority tasks eventually outrank fresh high-priority ones.
3. **Bounded Concurrency Everywhere**: Concurrency is strictly bounded at tenant, queue, and job-version scopes via atomic SQL counting and `FOR UPDATE SKIP LOCKED`.
4. **Least-Privilege Workers**: Worker tokens only satisfy `workers:claim`, `workers:heartbeat`, and `executions:write`. They cannot perform administrative mutations.
5. **Zombie Protection**: Worker crashes cannot strand tasks; expired leases are automatically reaped and stale worker completions are refused by the `CompletionGate`.
