# 24. Terminology

**Job** — reusable definition of work.

**Job version** — immutable executable configuration of a job.

**Schedule** — rule that creates executions.

**Execution** — one logical invocation of a job/workflow.

**Attempt** — one physical execution attempt for an execution.

**Task execution** — execution of one workflow node.

**Workflow** — DAG of dependent tasks/jobs.

**Worker** — process capable of executing work.

**Lease** — temporary authoritative execution ownership.

**Queue** — durable set of executable work items.

**Retry** — creation of another attempt after a failed attempt.

**Dead letter** — terminal state indicating the system will not automatically retry.

**Misfire** — scheduled occurrence that was not processed at its intended time.

**Catch-up** — processing missed scheduled occurrences.

**Concurrency limit** — maximum number of simultaneous executions.

**Capability** — worker property used for scheduling.

**Resource requirement** — requested CPU/memory/other capacity.

**Tenant** — isolated customer/organization boundary.

**Audit event** — immutable record of a security or administrative action.

**Outbox event** — persisted event waiting for publication.

**Correlation ID** — identifier linking related operations.

**Idempotency key** — caller-provided key allowing duplicate mutation requests to resolve safely.

**Recovery** — server process that handles abandoned work after lease expiry.

**Terminal state** — execution state from which no further normal transition occurs.
