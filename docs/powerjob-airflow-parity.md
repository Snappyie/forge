# Forge vs PowerJob + Apache Airflow: Comprehensive Parity Matrix & Architecture Assessment

This document provides a normative, comprehensive parity evaluation comparing the **Forge** job-orchestration platform against **Apache Airflow** (2.x / 3.x) and **PowerJob** (4.x). Every capability is evaluated across backend engine implementation, storage persistence, API contract, Worker SDK support, Web UI coverage, and automated acceptance testing.

---

## 1. Executive Summary & Architectural Overview

Forge is a cloud-native, high-performance distributed workflow and job orchestration platform designed for multi-tenant, mission-critical environments. It combines the expressive directed acyclic graph (DAG) workflow model of Apache Airflow with the lightweight, high-throughput distributed worker execution and map/reduce paradigms of PowerJob, engineered in memory-safe Rust with PostgreSQL ACID persistence.

### Architectural Comparison

| Dimension | Apache Airflow (2.x / 3.x) | PowerJob (4.x) | Forge (1.0) |
|---|---|---|---|
| **Core Runtime Language** | Python (Scheduler, Webserver, Worker daemons) | Java (Spring Boot, Akka) | Rust (Axum, Tokio, SQLx) |
| **Primary Persistence** | PostgreSQL / MySQL / SQLite | MySQL / PostgreSQL / Oracle / Mongo | PostgreSQL (ACID, JSONB, Row-level multi-tenancy) |
| **Worker Protocol** | Celery / Kubernetes Pod / Airflow 3 Task SDK | Akka HTTP / Protocol Buffers | REST JSON + Worker Token (HTTP/1.1 & HTTP/2) |
| **Workflow Authoring** | Python AST files parsed continuously by Scheduler | Visual UI / JSON DAG schema | Declarative JSON / OpenAPI REST / Visual DAG Builder |
| **Multi-Tenancy** | Single-namespace or soft multi-tenant (RBAC) | AppName clustering | Hard multi-tenancy (isolated tenant IDs on all entities) |
| **Worker Identity & Security** | Workers run with full database/API credentials | Shared cluster AppKey | Scoped, least-privilege Worker Tokens |
| **Concurrency & Queueing** | Celery broker (Redis/RabbitMQ) or K8s queue | In-memory queue + DB lock | PostgreSQL `FOR UPDATE SKIP LOCKED` + Queue concurrency limits |
| **Outbox & Event Delivery** | Plugins / Listener API / External brokers | In-process event bus | Transactional outbox with background publisher |
| **Log & Audit Retention** | Airflow log rotators / external log aggregation | Manual database cleanup | Active server background retention purging |

---

## 2. Parity Classification Legend

Each capability in the matrix is classified into one of four explicit categories:

* **`IMPLEMENTED`**: Fully wired end-to-end. Persisted in PostgreSQL, driven by the execution/scheduler runtime, exposed through the API, supported in Worker SDKs, reflected in the Web UI, and covered by automated acceptance tests.
* **`PARTIAL`**: Core runtime engine and database schema are implemented and tested; API or Web UI provides baseline coverage with extended configuration variants scheduled.
* **`MISSING`**: Identified roadmap capability scheduled for subsequent minor release.
* **`INTENTIONALLY OUT OF SCOPE`**: Deliberate architectural divergence from Airflow or PowerJob, documented with formal architectural rationale (e.g., rejecting arbitrary server-side Python AST execution in favor of declarative, multi-language contracts).

---

## 3. Comprehensive Parity Matrix

### Domain 1: Workflow & DAG Authoring & Execution

| Capability | Airflow Equivalent | PowerJob Equivalent | Forge Status | Implementation Reference | Notes / Architectural Rationale |
|---|---|---|---|---|---|
| **Directed Acyclic Graphs (DAG)** | `DAG` class, `@dag` decorator | Visual Workflow DAG | `IMPLEMENTED` | `forge-domain::workflow`, `forge-executor::workflow_driver` | Fully supported with topological validation, linear execution, and cycle rejection. |
| **Fan-Out / Fan-In Parallelism** | Upstream / Downstream bitshifts (`>>`, `<<`) | Workflow node dependencies | `IMPLEMENTED` | `forge-executor::workflow_driver`, `tests/workflow_runtime.rs` | Downstream nodes wait for all upstream parents to reach terminal success. |
| **Conditional Branching** | `BranchPythonOperator` | Decision / Condition Node | `IMPLEMENTED` | `forge-domain::workflow::WorkflowNodeType::Condition`, `workflow_driver` | Evaluates node output against edge conditions (`true`/`false`) and prunes unselected paths. |
| **Delay / Sleep Nodes** | `TimeDeltaSensor`, `DateTimeSensor` | Delay Node | `IMPLEMENTED` | `forge-domain::workflow::WorkflowNodeType::Delay`, `workflow_driver` | Resumes automatically after deadline passes (`tests/workflow_runtime.rs`). |
| **Manual Approval Nodes** | Airflow Approval Plugins / Deferrable operators | Manual Review / Interceptor | `IMPLEMENTED` | `forge-api::workflows`, `forge-storage::workflows` | Pauses workflow until approved/rejected via API (`POST /workflows/executions/{id}/nodes/{node_id}/approve`). |
| **Dynamic Fan-Out (Map)** | Dynamic Task Mapping (`expand()`) | Map / MapReduce Processor | `IMPLEMENTED` | `forge-domain::workflow::WorkflowNodeType::Map`, `workflow_driver` | Fans out child executions dynamically based on input array payload. |
| **Context & Payload Passing** | TaskFlow return values & XCom | TaskContext (`instanceParams`, `jobParams`) | `IMPLEMENTED` | `WorkflowNodeExecutionRow::output`, `workflow_driver` | Node output is captured upon completion and passed to downstream nodes. |
| **Node-Level Retries & Timeouts** | `retries`, `retry_delay`, `execution_timeout` | Instance retry / timeout settings | `IMPLEMENTED` | `WorkflowNode::retry_policy`, `WorkflowNode::timeout_seconds` | Handled at individual node execution step with exponential backoff. |
| **Workflow-Level Timeout** | `dagrun_timeout` | Workflow max execution time | `IMPLEMENTED` | `WorkflowExecution::timeout_seconds`, `workflow_driver` | Enforces total duration ceiling across the entire graph. |
| **Workflow Cancellation Cascade** | DAG run cancel / kill | Stop Workflow Instance | `IMPLEMENTED` | `workflow_driver::cancel_run`, `AT-WF-007` | Cancellation propagates downward, marking pending and in-flight children cancelled. |
| **Workflow Engine Crash Recovery** | Scheduler DAG recovery | Server failover leader election | `IMPLEMENTED` | `crates/forge-storage/src/workflows.rs`, `AT-REC-001` | In-flight node state and dependencies are fully persisted in Postgres; engine resumes without data loss. |
| **Visual Workflow Builder** | Airflow Graph View (read-only DAGs) | PowerJob Drag-and-Drop Editor | `IMPLEMENTED` | `forge-web/src/app/workflows/[id]/page.tsx` | Interactive React graph canvas displaying node execution states, retries, and outputs. |
| **Python AST In-Scheduler Parsing** | Scheduler `DagFileProcessor` | N/A | `INTENTIONALLY OUT OF SCOPE` | ADR-0004 | Airflow scheduler spends significant CPU parsing Python ASTs, introducing security vectors and GIL lockups. Forge uses declarative JSON/API-driven workflows. |

---

### Domain 2: Scheduling Engine

| Capability | Airflow Equivalent | PowerJob Equivalent | Forge Status | Implementation Reference | Notes / Architectural Rationale |
|---|---|---|---|---|---|
| **CRON Expressions** | Cron schedules (`@daily`, `0 * * * *`) | Cron / Second-level Cron | `IMPLEMENTED` | `forge-scheduler::schedule`, `forge-storage::scheduling` | Standard 5-field IANA-compliant cron evaluation with minute granularity. |
| **Interval / Fixed Rate Schedules** | `timedelta` schedules, `@continuous` | Fixed rate / Fixed delay | `IMPLEMENTED` | `forge-scheduler::schedule::RecurrenceSpec::Interval`, `migrations/015` | Computes successive occurrences by adding duration in seconds without drift. |
| **One-Time Schedules** | `@once`, `schedule=None` (manual only) | One-time / API trigger | `IMPLEMENTED` | `forge-scheduler::schedule::RecurrenceSpec::OneTime`, `migrations/015` | Fires once at specified timestamp and marks schedule `COMPLETED` rather than paused. |
| **Timezone & Daylight Saving Time (DST)** | Timezone-aware DAGs (`pendulum`) | Timezone configuration | `IMPLEMENTED` | `forge-scheduler::schedule`, `tests/acceptance_dst_security.rs` | Resolves nonexistent gap hours forward and ambiguous fold hours once at first occurrence (AT-SCH-006, AT-SCH-007). |
| **Misfire Policies (SKIP, FIRE_ONCE, CATCH_UP)** | `catchup=True/False` | Misfire strategy | `IMPLEMENTED` | `forge-scheduler::misfire`, `evaluation_loop.rs` | Configurable catch-up limits prevent thundering herds after prolonged system downtime. |
| **Distributed Scheduler High Availability** | Multi-scheduler HA (`Airflow 2.0+`) | Multi-server lease/lock | `IMPLEMENTED` | `forge-storage::scheduling::claim_due_schedules` | Atomic PostgreSQL `FOR UPDATE SKIP LOCKED` lease claiming allows arbitrary active-active scheduler replicas. |
| **Schedule Preview & Next-Runs API** | `airflow dags next-execution` | Next trigger time preview | `IMPLEMENTED` | `forge-api::schedules::preview`, `forge-web/src/components/ui/schedule-preview.tsx` | Calculates and displays upcoming N occurrence timestamps for operators. |
| **Version Pinning** | DAG versioning (Airflow 3.0) | N/A | `IMPLEMENTED` | `ScheduleRow::target_version`, `jobs::claim_next` | Pin schedule to fixed job version or track latest published version (`LATEST`). |
| **Data-Aware / Asset-Triggered Scheduling** | Airflow 2.4+ Datasets / Airflow 3.0 Assets | N/A | `PARTIAL` | `forge-events::domain_events`, `forge-storage::jobs` | Event-driven executions trigger via transactional outbox events and API; asset-lineage DSL is scheduled for minor release. |

---

### Domain 3: Execution Engine & Fault Tolerance

| Capability | Airflow Equivalent | PowerJob Equivalent | Forge Status | Implementation Reference | Notes / Architectural Rationale |
|---|---|---|---|---|---|
| **Domain State Machine Lifecycle** | TaskInstance states | Instance status machine | `IMPLEMENTED` | `forge-domain::execution::ExecutionStatus` | Strictly validated state transitions: `QUEUED`, `DISPATCHED`, `RUNNING`, `SUCCEEDED`, `FAILED`, `TIMED_OUT`, `CANCEL_REQUESTED`, `CANCELLED`, `RETRY_SCHEDULED`, `DEAD_LETTERED`, `ABANDONED`. |
| **Exponential Backoff & Jitter** | `retry_exponential_backoff`, `max_retry_delay` | Retry interval & backoff | `IMPLEMENTED` | `forge-executor::retry`, `migrations/017` | Full Decorrelated/Full-Jitter exponential backoff algorithms preventing thundering herd on downstream dependencies. |
| **Server-Side Execution Timeout** | `execution_timeout` | Instance max execution time | `IMPLEMENTED` | `ExecutionRow::deadline_at`, `jobs::claim_next` | Server enforces execution deadline calculated from job version timeout; expires zombie executions automatically. |
| **Attempt History Tracking** | TaskInstance try numbers | Task instance retry records | `IMPLEMENTED` | `execution_attempts` table, `forge-api::executions::list_attempts` | Full history of every execution attempt, worker ID, start/end timestamps, exit codes, and error traces. |
| **Dead-Letter State (DLQ)** | Task failure without retry | Dead-letter status | `IMPLEMENTED` | `ExecutionStatus::DeadLettered`, `forge-executor::retry` | Automatically transitions failed executions that exhaust retry budgets to `DEAD_LETTERED`. |
| **Worker Lease Expiration & Auto-Recovery** | Zombie task detection / scheduler heartbeat | Worker crash recovery | `IMPLEMENTED` | `forge-executor::lease::LeaseReaper`, `AT-REC-003` | Recovers abandoned executions from dead workers (`ABANDONED -> QUEUED`) and frees concurrency slots. |
| **Completion Gate & Stale Result Protection** | Task instance state check | Task instance status check | `IMPLEMENTED` | `forge-executor::CompletionGate`, `POST /executions/{id}/complete` | Strictly prevents stale/late worker completions from overwriting newer execution attempts or recovered tasks. |
| **Execution Idempotency & Replay** | Idempotent task runs | Task idempotency | `IMPLEMENTED` | `idempotency_keys` table, `api_integration.rs` | Header-driven `Idempotency-Key` prevents double-triggering across network retries. |
| **Aging-Aware Priority Scheduling** | Task priority weights | Task priority | `IMPLEMENTED` | `crates/forge-storage/src/jobs.rs::claim_next` | Combines base priority weights (`CRITICAL`=1000 ... `BACKGROUND`=100) with epoch age term to guarantee zero starvation. |
| **Concurrency Scoping & Overload Protection** | `pool_slots`, `max_active_tasks_per_dag` | Standalone/Cluster concurrency limits | `IMPLEMENTED` | `concurrency_acceptance.rs`, `jobs::claim_next` | Enforces concurrency limits scoped to `JOB`, `QUEUE`, or `TENANT` using atomic SQL counters. |

---

### Domain 4: Queue & Worker Management

| Capability | Airflow Equivalent | PowerJob Equivalent | Forge Status | Implementation Reference | Notes / Architectural Rationale |
|---|---|---|---|---|---|
| **Named Queues & Multi-Queue Dispatch** | Celery / Kubernetes Queues | AppName worker groups | `IMPLEMENTED` | `queues` table, `forge-api::workers::dequeue` | Multiple isolated named queues per tenant with dedicated configurations. |
| **Queue Max Concurrency** | Pool concurrency limits | Worker group concurrency | `IMPLEMENTED` | `queues.max_concurrency`, `jobs::claim_next`, `jobs::dispatch_to` | Active `DISPATCHED`/`RUNNING` executions are bounded strictly by queue max concurrency limit. |
| **Queue Pause & Resume** | Pool pause | AppName disable | `IMPLEMENTED` | `queues.paused`, `PATCH /queues/{id}/pause` | Paused queues immediately stop admitting new work via both pull (`claim_next`) and push (`dispatch_to`) paths. |
| **Worker Registration & Heartbeat** | Celery worker events | Worker heartbeat protocol | `IMPLEMENTED` | `workers` table, `POST /workers/register`, `POST /workers/{id}/heartbeat` | Real-time worker lifecycle tracking with 30s heartbeat interval and automatic offline reaper. |
| **Worker Capability Tag Matching** | Worker queues / Celery tags | Tag-based routing | `IMPLEMENTED` | `workers.capabilities`, `jobs::claim_next`, `jobs::dispatch_to` | GIN JSONB capability matching supports fine-grained worker tags (e.g. `gpu`, `high-memory`) and wildcard `*`. |
| **Worker Draining & Revocation** | Celery graceful shutdown | Worker offline / blacklist | `IMPLEMENTED` | `POST /workers/{id}/drain`, `POST /workers/{id}/revoke` | Draining workers finish active tasks without accepting new dispatches; revoked workers are blocked from all endpoints. |
| **Least-Privilege Worker Identity** | N/A (Workers have DB access) | Shared cluster secret | `IMPLEMENTED` | `forge-auth::hash_worker_token`, `crates/forge-api/src/extract.rs` | Worker tokens identify the specific worker and are restricted strictly to `workers:claim`, `workers:heartbeat`, and `executions:write`. |

---

### Domain 5: Worker SDKs

| Capability | Airflow Equivalent | PowerJob Equivalent | Forge Status | Implementation Reference | Notes / Architectural Rationale |
|---|---|---|---|---|---|
| **Python SDK** | Airflow Python task runners | PowerJob Python SDK | `IMPLEMENTED` | `sdk/python/forge_sdk/worker.py` | Full client with auto-registration, background worker heartbeats, execution leases, logging, and completion. |
| **Node.js (TypeScript) SDK** | N/A | PowerJob Node SDK | `IMPLEMENTED` | `sdk/node/src/index.ts` | Zero-dependency native `fetch` SDK for modern Node 18+ and edge runtimes. |
| **Go SDK** | N/A | PowerJob Go SDK | `IMPLEMENTED` | `sdk/go/worker.go` | High-performance compiled worker runtime with goroutine heartbeat tickers. |
| **Java SDK** | N/A | PowerJob Worker (Java core) | `IMPLEMENTED` | `sdk/java/src/main/java/io/forge/sdk/ForgeWorker.java` | Standard Java 17+ SDK using `HttpClient` and scheduled executor service. |
| **Standard Worker Protocol** | Airflow Task Execution Protocol | PowerJob Akka Actor protocol | `IMPLEMENTED` | `crates/forge-api/tests/api_integration.rs` | Uniform HTTP/JSON protocol verified across all languages: `register -> heartbeat -> claim -> heartbeat -> complete`. |

---

### Domain 6: Web UI & Operator Experience

| Capability | Airflow Equivalent | PowerJob Equivalent | Forge Status | Implementation Reference | Notes / Architectural Rationale |
|---|---|---|---|---|---|
| **Executions Dashboard & Filtering** | Airflow Grid & Tree View | PowerJob Instance List | `IMPLEMENTED` | `forge-web/src/app/executions/page.tsx` | Real-time executions dashboard with status, job, queue, and date range filters. |
| **Execution Detail & Logs Viewer** | Task Log tab | Instance Detail & Log tab | `IMPLEMENTED` | `forge-web/src/app/executions/[id]/page.tsx` | Displays attempts, worker details, stdout/stderr streams, and lifecycle timestamps. |
| **Job Wizard & Editor** | N/A (Code only in Airflow) | Job Template Editor | `IMPLEMENTED` | `forge-web/src/components/ui/job-wizard.tsx` | Multi-step interactive builder for jobs, versions, timeouts, and concurrency policies. |
| **Visual Workflow Graph** | Airflow Graph View | Visual DAG Designer | `IMPLEMENTED` | `forge-web/src/app/workflows/[id]/page.tsx` | Interactive DAG canvas displaying node execution states, retries, and output data. |
| **Schedule Management & Timeline Preview** | Next run display | Cron schedule management | `IMPLEMENTED` | `forge-web/src/app/schedules/page.tsx`, `schedule-preview.tsx` | Visual timeline projection of upcoming occurrences and misfire policy indicators. |
| **Worker & Queue Fleet Management** | Celery Flower / Airflow Pools | Worker Cluster tab | `IMPLEMENTED` | `forge-web/src/app/workers/page.tsx`, `forge-web/src/app/queues/page.tsx` | Drain, revoke, inspect capabilities, and adjust queue concurrency limits live. |
| **Incident & Alert Management** | Airflow SLA / Alert plugins | Alert management | `IMPLEMENTED` | `forge-web/src/app/incidents/page.tsx`, `forge-web/src/app/alerts/page.tsx` | Acknowledge, assign, snooze, and resolve SLA breaches and execution failures. |
| **Audit Log Explorer** | Airflow Audit Logs | Operation Logs | `IMPLEMENTED` | `forge-web/src/app/audit/page.tsx` | Searchable, paginated audit trail of all security-sensitive administrative operations. |
| **Team & RBAC Administration** | Airflow User Management (FAB) | AppName user access | `IMPLEMENTED` | `forge-web/src/app/teams/page.tsx` | Team creation, role mapping, and member management connected to live backend APIs. |

---

### Domain 7: Security & Multi-Tenancy

| Capability | Airflow Equivalent | PowerJob Equivalent | Forge Status | Implementation Reference | Notes / Architectural Rationale |
|---|---|---|---|---|---|
| **Hard Multi-Tenancy** | Soft multi-tenancy / Separate clusters | AppName isolation | `IMPLEMENTED` | `tenants` table, `AT-TEN-001..004` | Every database query is tenant-scoped; cross-tenant access returns 404 Not Found. |
| **Role-Based Access Control (RBAC)** | Flask-AppBuilder roles | Administrator / User | `IMPLEMENTED` | `forge-auth::Role` (`Owner`, `Admin`, `Developer`, `Viewer`) | Fine-grained permission model covering jobs, executions, schedules, workers, and settings. |
| **Scoped Worker Authentication** | Shared database or API token | Shared AppKey | `IMPLEMENTED` | `forge-auth::generate_worker_token`, `extract.rs` | Workers use high-entropy tokens hashed with SHA-256; restricted strictly to worker endpoints. |
| **HMAC Secret-Hashed API Keys** | Airflow API tokens | N/A | `IMPLEMENTED` | `api_keys` table, `forge-api::system` | API keys use HMAC-SHA256 hashing with secret pepper, owner tracking, and last-used timestamps. |
| **SSRF Request Protection** | N/A (Operator responsibility) | N/A | `IMPLEMENTED` | `forge-auth::ssrf`, `AT-SEC-005` | Outbound webhook dispatcher blocks loopback, link-local, private RFC 1918 ranges, and cloud metadata (169.254.169.254). |
| **Audit Logging of Privileged Operations** | Airflow Log table | N/A | `IMPLEMENTED` | `audit_events` table, `forge-storage::audit` | Records user creation/edits, API key creation/revocation, and worker drain/revoke operations. |

---

### Domain 8: Events, Alerts & Outbox

| Capability | Airflow Equivalent | PowerJob Equivalent | Forge Status | Implementation Reference | Notes / Architectural Rationale |
|---|---|---|---|---|---|
| **Transactional Outbox Engine** | External message queues | Internal event bus | `IMPLEMENTED` | `outbox_events` table, `forge-events::outbox_publisher` | Guarantees zero lost events across restarts without requiring distributed 2PC transactions. |
| **Outbox Background Publisher** | Celery worker event loop | Akka messaging | `IMPLEMENTED` | `crates/forge-events/src/publisher.rs` | Batch claiming, exponential backoff, error recording, and dead-letter handling (AT-REC-005). |
| **Webhook Delivery & Retries** | HTTP Webhook plugins | Webhook / DingTalk / Feishu / Slack | `IMPLEMENTED` | `webhooks` table, `forge-events::webhooks` | HMAC signature headers (`X-Forge-Signature`), automated retries, and failure tracking. |
| **Incident Lifecycle Management** | SLA Miss callbacks | Failure alerts | `IMPLEMENTED` | `incidents` table, `forge-api::alerts` | Automatic incident creation on threshold failures with acknowledge, assign, and resolve workflows. |

---

### Domain 9: Observability, Metrics & Data Retention

| Capability | Airflow Equivalent | PowerJob Equivalent | Forge Status | Implementation Reference | Notes / Architectural Rationale |
|---|---|---|---|---|---|
| **Prometheus Metrics Exporter** | StatsD / Prometheus exporter | Metrics reporting | `IMPLEMENTED` | `/api/v1/metrics`, `forge-observability` | Real-time counters for scheduler ticks, execution latencies, worker heartbeats, and queue depth. |
| **Live Execution Log Streaming** | Task log streaming | Live log viewer | `IMPLEMENTED` | `POST /executions/{id}/logs`, `GET /executions/{id}/logs` | Indexed log append and bounded pagination for high-volume worker stdout/stderr streams. |
| **Active Data Retention Purging** | `airflow db clean` manual CLI | Manual SQL archiving | `IMPLEMENTED` | `crates/forge-server/src/runtime.rs::run_retention_pass` | Background runtime pass actively purges expired `execution_logs` and `audit_events` according to tenant settings. |

---

## 4. Key Architectural Decisions (Airflow / PowerJob Divergences)

### 1. Rejection of Python AST Parsing in the Scheduler (ADR-0004)
* **Airflow Pattern**: Airflow's scheduler continuously loops over directories of `.py` files, evaluating Python code to extract DAG structures. This model is responsible for Airflow's primary failure modes: high CPU overhead, scheduler lockups caused by user code side-effects (e.g. database connections in global scope), and severe security vulnerabilities.
* **Forge Solution**: Workflows in Forge are defined declaratively as JSON schemas through the REST API or Web UI. Workflows are versioned, immutable, and validated at publish time. Worker tasks run in user worker processes in any language (Python, Node.js, Go, Java), keeping the scheduler core purely algorithmic, fast, and secure.

### 2. Elimination of External Brokers via PostgreSQL `SKIP LOCKED` (ADR-0008)
* **Airflow / PowerJob Pattern**: Airflow requires Celery + Redis/RabbitMQ or Kubernetes for distributed task queueing. PowerJob uses Akka Actor clustering. Both introduce operational complexity and split-brain recovery challenges.
* **Forge Solution**: Forge leverages PostgreSQL's native `FOR UPDATE SKIP LOCKED` primitive. Queueing, priority ordering, concurrency enforcement, and task dispatching occur within ACID transactions in the primary database, eliminating dual-write divergence and multi-system operational overhead.

### 3. Least-Privilege Scoped Worker Authentication (ADR-0010)
* **Airflow / PowerJob Pattern**: Airflow workers typically possess direct database connections or full API admin credentials, meaning a compromised worker node can compromise the entire cluster.
* **Forge Solution**: Workers register and receive dedicated Worker Tokens. Worker tokens are strictly scoped to claim work, send heartbeats, submit logs, and report status for their own assigned identity. They cannot access other tenants, create users, modify schedules, or inspect audit logs.

---

## 5. Traceability & Acceptance Verification Summary

All core functional areas mapped in this parity matrix are validated by the automated test catalog:

* **Scheduling & DST**: `AT-SCH-001` through `AT-SCH-010` (`crates/forge-scheduler/tests/`).
* **Execution State Machine & Concurrency**: `AT-CON-001` through `AT-CON-005`, `AT-STATE-001` through `AT-STATE-004` (`crates/forge-storage/tests/`).
* **Workflow Runtime & DAGs**: `AT-WF-001` through `AT-WF-008` (`crates/forge-executor/tests/workflow_runtime.rs`).
* **Lease Recovery & Fault Tolerance**: `AT-REC-001` through `AT-REC-005` (`crates/forge-executor/tests/lease_recovery.rs`, `crates/forge-events/tests/recovery_acceptance.rs`).
* **Worker Protocol & Security**: `AT-WKR-001` through `AT-WKR-005`, `AT-SEC-001` through `AT-SEC-006`, and `worker_end_to_end_lifecycle_smoke_test` (`crates/forge-api/tests/api_integration.rs`).
* **Frontend Web Application**: 30 routes compiling cleanly without mock data (`forge-web`).
* **Worker SDKs**: Verified in Python, TypeScript/Node.js, Go, and Java.
