# 14. Testing Strategy

## 14.1 Test pyramid

1. Unit tests.
2. Domain/property tests.
3. Repository integration tests.
4. API integration tests.
5. Worker integration tests.
6. Workflow tests.
7. End-to-end tests.
8. Load tests.
9. Failure-injection tests.
10. Security tests.

## 14.2 Domain tests

Must cover every:
- valid transition;
- invalid transition;
- retry calculation;
- schedule calculation;
- timezone;
- DST;
- concurrency rule;
- workflow graph validation.

## 14.3 Property tests

Examples:
- generated valid DAGs remain acyclic after valid transformations;
- retry delays never exceed max delay;
- execution IDs are unique;
- state machine never reaches invalid state;
- schedule preview equals actual scheduler calculation for same clock/configuration.

## 14.4 Repository tests

Run against real PostgreSQL in CI.

Test:
- transaction rollback;
- unique constraints;
- tenant isolation;
- row locks;
- concurrent claiming;
- migrations;
- retention.

## 14.5 Concurrency tests

Test:
- two schedulers claiming same schedule;
- two workers claiming same execution;
- lease renewal races;
- stale completion;
- cancellation vs completion;
- retry vs timeout;
- duplicate API requests.

## 14.6 Failure injection

Simulate:
- worker crash;
- database disconnect;
- API restart;
- scheduler restart;
- network delay;
- duplicate message;
- dropped completion acknowledgement;
- clock jump in test clock;
- outbox publication failure.

## 14.7 Security tests

- broken access control;
- tenant escape attempts;
- SSRF;
- command injection;
- malformed JWT/session;
- API key reuse after revocation;
- rate limiting;
- log secret leakage;
- path traversal;
- oversized payloads.

## 14.8 UI tests

Test:
- create job;
- edit job;
- publish;
- schedule preview;
- trigger;
- cancel;
- retry;
- workflow creation;
- worker drain;
- audit filtering;
- permission restrictions.

## 14.9 Load testing

Reference scenarios:
- 1k executions/minute;
- 10k queued executions;
- 100 workers;
- 1k jobs;
- 10k schedules;
- workflow fan-out;
- high failure/retry rate.

Publish results as versioned benchmark reports.

## 14.10 Acceptance criteria

A feature is not complete until:
- tests exist;
- failure behavior is documented;
- observability exists;
- API/UI behavior is documented;
- security implications are assessed;
- migration is tested if applicable.
