# 22. Architecture Decision Register

## ADR-0001 Rust as primary implementation language

Status: Accepted.

Decision:
Use Rust for server/domain/scheduler/worker components.

Reason:
The project is intended to exercise memory safety, concurrency, performance, and reliable systems programming.

## ADR-0002 PostgreSQL as V1 source of truth

Status: Accepted.

Decision:
PostgreSQL is authoritative for execution state.

Reason:
Strong transactions, locking, indexing, operational maturity.

## ADR-0003 Safe Rust by default

Status: Accepted.

Decision:
Forbid unsafe unless explicitly approved.

## ADR-0004 At-least-once execution semantics

Status: Accepted.

Decision:
The platform provides at-least-once infrastructure semantics and idempotency primitives.

Reason:
Exactly-once external side effects cannot be universally guaranteed.

## ADR-0005 Versioned immutable job definitions

Status: Accepted.

Decision:
Published versions cannot mutate.

Reason:
Reproducibility and auditability.

## ADR-0006 Domain/infrastructure separation

Status: Accepted.

Decision:
Core domain must not depend on HTTP/database frameworks.

Reason:
Testability and long-term maintainability.

## ADR-0007 Open-source licensing

Status: Accepted.

Decision:
MIT OR Apache-2.0 for code unless a later legal review selects another compatible license.

## ADR-0008 API versioning

Status: Accepted.

Decision:
Public API is versioned from its first release.

## ADR-0009 Workflow DAG model

Status: Accepted.

Decision:
V1 workflows are directed acyclic graphs.

Reason:
Deterministic dependency semantics and manageable execution model.

## ADR-0010 Durable outbox

Status: Accepted.

Decision:
Use an outbox for reliable event publication.

## ADR-0011 Execution `ABANDONED` is an intermediate state, not terminal

Status: Accepted.

Amends: 02-domain-model.md §2.5/§2.6.

Decision:
`ABANDONED` is not a terminal state. The permitted transitions out of it are
`ABANDONED -> QUEUED` (recovery re-queues the work) and
`ABANDONED -> DEAD_LETTERED` (recovery gives up).

Reason:
The specification's own transition table lists `ABANDONED -> QUEUED`, and §2.5
states that `ABANDONED` is an intermediate recovery state unless product policy
makes it terminal. Treating it as terminal makes that required transition
unreachable, so a lost worker would strand its execution permanently — in
direct conflict with the "no silent job loss" design principle in README.md.

`RECOVERED`, which appears in the §2.5 diagram, is not a state: it has no entry
in the §2.6 transition table and no terminal classification. It is an artifact
of the diagram and is not implemented.

## ADR-0012 `FAILED` and `TIMED_OUT` permit retry and dead-letter exits

Status: Accepted.

Amends: 01-product-requirements.md invariant 3; 02-domain-model.md §2.6.

Decision:
A terminal execution may not return to an active state, with two documented
exceptions: `FAILED -> RETRY_SCHEDULED` and `FAILED -> DEAD_LETTERED`, plus the
equivalent pair for `TIMED_OUT`.

Reason:
Invariant 3 forbids terminal executions from becoming active again, while §2.6
requires retry and dead-letter transitions out of `FAILED`. Reading the
invariant as absolute would make the retry table unreachable. The exception is
narrow: neither transition re-enters `QUEUED`, `DISPATCHED` or `RUNNING`, so
the invariant's intent is preserved.

## ADR-0013 Five-field cron dialect

Status: Accepted.

Amends: 09-scheduling-engine.md §9.4.

Decision:
V1 recurring schedules use standard five-field cron syntax
(`minute hour day-of-month month day-of-week`). Six- and seven-field
extensions are rejected rather than inferred. Days of the week are numbered
from **Sunday**, where `0` is Sunday and `1` is Monday.

Reason:
§9.4 explicitly defers the dialect choice but forbids silently supporting
ambiguous dialects. Pinning one dialect and documenting it in the OpenAPI
specification satisfies both halves of that requirement.

Day-of-week numbering needs stating separately because the two conventions in
common use disagree: the Unix convention treats `1` as Monday, whereas the
underlying parser numbers from Sunday, so `1-5` means Sunday through Thursday
rather than "weekdays". A schedule written under the Unix reading would fire on
the wrong days. The OpenAPI description MUST carry this explicitly, and
`forge_scheduler::schedule` tests pin the behaviour so it cannot drift.

Implementation note: the parser takes a *six*-field expression beginning with
seconds. Forge's dialect omits seconds, so a fixed `0 ` is prepended during
parsing. Passing the raw string through would read `0 2 * * *` as
second=0/minute=2, shifting every occurrence by one field.

## ADR-0014 Ambiguous local times fire once, at the first occurrence

Status: Accepted.

Amends: 09-scheduling-engine.md §09.6.

Decision:
Where a local time occurs twice during a DST fall-back, the schedule fires
once, at the first (earlier) occurrence. Nonexistent local times during a
spring-forward shift forward past the gap, and the schedule's misfire policy
then decides whether the shifted instant still fires.

Reason:
§09.6 requires an explicitly documented first/second policy. Firing once
avoids the duplicate execution that a "both" policy would produce, and the
database constraint on `(schedule_id, scheduled_for)` would reject the second
write anyway.

Implementation note: the underlying cron iterator walks local wall-clock time
and steps *over* an ambiguous hour rather than emitting both candidates. That
is compatible with this ADR — one wall-clock time yields at most one execution —
but the practical consequence is that a schedule targeting the repeated hour
does not run at all on the fall-back day. This is documented operator-facing
behaviour, not a silent surprise.

## ADR-0015 A schedule's timezone is required

Status: Accepted.

Amends: 09-scheduling-engine.md §9.5.

Decision:
A recurring schedule without an IANA timezone is rejected, unless
`FORGE_DEFAULT_TIMEZONE` is set explicitly, in which case that value is applied
and recorded on the schedule. The built-in default for the setting is `UTC`.

Reason:
§09.5 prohibits silent machine-local timezone behaviour. Making the deployment
default explicit and persisting it keeps schedules reproducible across hosts.

## ADR-0016 Cancellation is cooperative only

Status: Accepted.

Amends: 10-execution-engine.md §10.6.

Decision:
V1 provides no hard-kill policy for cancellation. The server records the request,
the worker observes it, stops, cleans up and acknowledges, and the server-side
timeout remains authoritative.

Reason:
§10.6 states a hard-kill policy MAY exist but must be explicit. Omitting it
entirely is explicit, and avoids implying a containment guarantee that a
container executor cannot make across arbitrary runtimes.

## ADR-0017 Runtime SQL queries instead of compile-time macros

Status: Accepted.

Decision:
All SQL uses `sqlx::query_as::<_, T>` with `#[derive(FromRow)]`. The
compile-time `query!`/`query_as!` macros are not used, and no `.sqlx` offline
cache is committed.

Reason:
A committed query cache must be regenerated whenever a query or schema changes.
Without it, a clean checkout or a CI clone cannot compile at all, which
contradicts the README requirement that contributors can run the complete stack
locally. Row-shape structs give the same type safety at the boundary.

## ADR-0018 Status vocabularies use CHECK constraints, not native enums

Status: Accepted.

Decision:
Status columns are `VARCHAR` plus a named `CHECK` constraint.

Reason:
Adding a value to a PostgreSQL enum requires DDL that cannot run inside a
transaction in older server versions, which conflicts with the forward-only
migration requirement in 08-storage-specification.md §8.1. CHECK constraints are
dropped and recreated in place.

## ADR-0019 Tenant slugs are addressable identifiers, derived by trigger

Status: Accepted.

Amends: 05-api-specification.md (no tenant addressing existed); 08-storage-specification.md §8.

Decision:
A tenant gains a `slug` that is unique, non-null, and stable for the life of the
tenant. It is the HTTP-facing identifier (`/tenants/{slug}`). A `BEFORE INSERT`
trigger fills it from the tenant id when the caller does not supply one.

Reason:
A tenant was previously reachable only through a bearer claim, with no route that
addresses one. `redesign.md` §G requires per-tenant isolation to be an operable
model, and an operator cannot administer, switch between, or audit a tenant that
has no name they can type.

The slug is derived rather than required at every call site because several code
paths create a tenant without naming one; making the value mandatory in
application code is a slug some caller will eventually forget, which would fail at
runtime rather than at compile time.

Implementation note:
The value is set by a trigger, not a `DEFAULT`. Postgres rejects a column
reference in a `DEFAULT` expression (`cannot use column reference in DEFAULT
expression`) — including indirectly through a function call — so a slug derived
from the row's own id is not expressible as a default. This was found by applying
the migration to an empty database; it passes on a populated one only because no
row is inserted while the column still allows null.

## ADR-0020 An `ON CONFLICT` target must be backed by a unique index

Status: Accepted.

Amends: 06-event-and-queue-contracts.md; 10-execution-engine.md §10.2.

Decision:
Every `INSERT ... ON CONFLICT` in the storage layer names a column set that a
unique index or constraint actually backs. Adding a conflict target and its
index is one change, and a migration that adds an `ON CONFLICT`-dependent column
set adds the index with it.

Reason:
`ExecutionRepository::create` provisions a tenant's `default` queue with
`ON CONFLICT (tenant_id, name) DO NOTHING`, but no migration ever created that
unique index. Postgres requires one to arbitrate the conflict, so every
execution created for a tenant without a pre-existing `default` queue failed with
`42P10`. The SELECT that runs first hid the defect on a seeded database and only
surfaced on a fresh one — 35 integration tests across four suites failed while
the same tests passed against a database that already had a `default` queue.

Beyond the outage, the missing index also removed the guarantee the surrounding
comment claimed: without it, two concurrent inserts can both succeed and the
follow-up `fetch_one` becomes a race.

## ADR-0021 Tenant isolation is enforced by the database, not only by the query

Status: Accepted.

Amends: 11-security.md; 08-storage-specification.md §8.

Decision:
Every table holding tenant data has Postgres row-level security enabled with a
policy `tenant_id = current_tenant_id()`, where `current_tenant_id()` reads
`SET LOCAL app.current_tenant`. The existing application-level `WHERE tenant_id`
predicates are kept.

Reason:
Until now isolation was a *convention*: every repository method took a
`tenant_id` and put it in the query. That is a real boundary, but a single
query that forgets the predicate reads another tenant's data, and no test would
catch it unless that exact query ran with two tenants present.

RLS holds for every writer — psql, a migration, a future service, and the
queries in this codebase nobody has read yet — which is the property the
convention cannot offer. Keeping both checks is what makes this defence in depth
rather than a race to delete the now-redundant one.

Implementation notes an operator needs:

- The transaction sets the tenant once: `SET LOCAL app.current_tenant = '<uuid>'`.
  `SET LOCAL` scopes it to the transaction, so it cannot leak to the next request
  on a pooled connection. A bare `SET` would.
- An unset variable **denies every row** rather than erroring, so a code path
  that forgets to set it fails closed.
- **RLS does not apply to the table owner.** An application connecting as the
  owner gets no protection. Migration 020 creates a `forge_app` role for this,
  but deliberately does not `GRANT`: which tables a given installation exposes
  is its own decision, and a migration asserting a fixed set would silently
  over-grant after a future table is added.

  **This is an operator action.** Until the application connects as `forge_app`,
  the policies are inert and isolation still depends on the query predicates.
- `users` is intentionally not scoped: a user row is reachable through
  `tenant_memberships`, so scoping `users` by tenant would make an operator
  unable to see or disable a colleague who has not yet joined the tenant.

## ADR-0022 An application and an environment are separate containers

Status: Accepted.

Amends: 02-domain-model.md §2.1-2.2; 11-security.md §11.

Decision:
A job belongs to an `application` (what the work is) and an `environment` (where
it runs) as independent containers. Application slugs are unique per tenant;
a tenant has at most one environment of kind `production`.

Reason:
A job's identity — name, key, schedule, payload shape — is the same in dev and
prod. Its bindings — queue, worker pool, secret names — are not. Migration copies
the former and re-points the latter, which is only expressible if the two sides
are independently addressable. One generic "group" entity could not represent
that split.

At most one production environment per tenant is a correctness constraint rather
than tidiness: two environments both marked production would mean a change
guardrail protects one and silently skips the other — the "operator believes it
is isolated and it is not" failure that `EnvironmentKind` exists to prevent.

## ADR-0023 Single sign-on is OpenID Connect authorization-code with PKCE

Status: Accepted.

Amends: 11-security.md §11.2 (passwords only); 08-storage-specification.md §8.2.

Decision:
Federated login uses OpenID Connect authorization code with PKCE (`S256`).
The ID token is verified against the provider's JWKS, checking signature,
`iss`, `aud` and `exp`. An identity provider is registered in the database, and
a provider may be restricted to a list of email domains.

Reason:
`redesign.md` §G requires SSO "through OIDC/OAuth2 and enterprise identity
providers", and the `user_identities` table had existed since migration 005
without a line of code reading it.

Three decisions are load-bearing rather than incidental:

- **PKCE is not optional.** The authorization code travels through the browser.
  A confidential client keeps its secret server-side, but without PKCE anyone
  who observes the redirect can redeem the code. RFC 9700 requires it.
- **The ID token is never decoded without verification.** An unverified ID
  token is an attacker-chosen user identity. There is deliberately no
  "unverified" code path, and an unexpected `alg` is refused rather than skipped
  — silently declining to verify is how `alg: none` bypasses happen.
- **The discovery document's `iss` must match what was configured.** Otherwise a
  hijacked DNS entry can redirect the whole flow at a substitute IdP whose
  tokens would then verify.

Domain restriction is the control that stops a misconfigured IdP from granting
access to arbitrary accounts: a provider limited to verified domains refuses an
identity outside them instead of provisioning one.

## ADR-0024 An external identity is not tenant-scoped

Status: Accepted.

Amends: 08-storage-specification.md §8.2.

Decision:
`user_identities` is keyed on `(issuer, provider, provider_subject)` and carries
no `tenant_id`.

Reason:
Migration 005 declared `(provider, subject)` unique **per tenant**, which is
wrong: an identity provider's `sub` identifies a person, and one person may be a
member of several tenants. Keying per tenant would let the same provider
identity be recorded once per tenant with nothing tying the duplicates to one
`user_id`, so a login would resolve to whichever tenant the row happened to
carry.

The `issuer` is part of the key because two providers can each issue a `sub` of
`42`; without it, any deployment configured with more than one provider collides
on the first login.

## ADR-0025 A tenant switch issues a new session

Status: Accepted.

Amends: 05-api-specification.md (no tenant addressing); 11-security.md §11.

Decision:
`POST /auth/switch-tenant` returns a **new** access/refresh pair rather than
mutating the current one. Login accepts an optional `tenant_slug`; when omitted
the user gets the tenant they last used, never an arbitrary one.

Reason:
The login query was a `LEFT JOIN tenant_memberships ... LIMIT 1` with no
ordering, so a user in several tenants landed in whichever row the planner
returned first — varying between deployments and between restarts. A login that
silently picks a tenant is unmanageable in exactly the deployments that need
multi-tenancy.

Returning a new token pair follows from the token's shape: an access token
carries exactly one `tenant_id`, so a switch is a new session by construction.
Mutating in place would leave the previously issued token valid for a tenant the
user has since left.

`users.last_tenant_id` is guarded by a trigger so it can only ever point at a
tenant the user is a member of; otherwise a revoked membership would leave an
operator defaulting into a tenant they can no longer enter.

## ADR-0026 Service accounts carry scopes, not a role

Status: Accepted.

Amends: 11-security.md §11.4; 02-domain-model.md §2.

Decision:
A service account is a non-human principal with an explicit list of
`resource:action` scopes. An empty scope list means no access, not full access.

Reason:
Service accounts are neither users nor workers, and both of those models were
already spoken for: a user has a password or an SSO identity, and a worker may
only claim and complete its own work. A deployment token is a third thing —
"deploy the production jobs from CI" and "read the audit log" are different
grants, and a role would imply one implies the other.

Defaulting an empty list to "everything" is the failure this avoids: an account
created without scopes would otherwise be maximally privileged, which is the
opposite of the intent.

## ADR-0027 The OIDC client secret is encrypted, not hashed

Status: Accepted.

Amends: 08-storage-specification.md §8.2; 11-security.md §11.

Decision:
`identity_providers.client_secret_ciphertext` holds an AES-256-GCM
ciphertext. The key is derived from the existing
`FORGE_API_KEY_HASHING_SECRET` with SHA-256, and a fresh 96-bit nonce is drawn
per encryption. The plaintext is never stored and never returned by any
endpoint.

Reason:
The first implementation hashed the secret, mirroring the `api_keys` pattern —
which is correct for a credential the server only ever *compares*. An OIDC
client secret is the opposite case: the server must present it verbatim to the
provider's token endpoint on every login, so a one-way hash makes the flow
impossible while still leaving the column populated. A plaintext column would
put a credential in every database dump, so the value is encrypted instead.

GCM rather than a bare cipher because the plaintext is a credential that goes
straight into an outbound HTTPS request: a tampered ciphertext must fail to
decrypt rather than decrypt into a different secret. `forge-auth/src/crypto.rs`
tests that a modified nonce and a modified ciphertext are both rejected.

Reusing the API-key pepper avoids introducing a second secret that an operator
has to remember to configure, and matches how the deployment already treats
that value.

## ADR-0028 An OIDC callback can only be reached with a server-issued state token

Status: Accepted.

Amends: ADR-0023; ADR-0024; 05-api-specification.md.

Decision:
`GET /auth/oidc/callback` accepts `code` and `state`. The `state` token is
consumed by a single `DELETE ... RETURNING` on `oidc_pending_logins`, which is
what makes it both validated and single-use. The callback is `GET` only. The
identity comes exclusively from the JWKS-verified ID token. An unlinked
identity may join an existing account whose email matches the verified claim
exactly, and may never create one.

Reason:
The first implementation of these endpoints issued a session from a
`POST /auth/oidc/callback` body, reading `subject` and `email` straight out of
it. Verified live against a running server: an unauthenticated request minted a
working session (`GET /jobs` → 200), and posting an existing administrator's
email returned **that account's `user_id` with role `ADMIN`** — account takeover
from an anonymous POST. It also contradicted three committed decisions:
`state` was generated and never checked, so it was not a CSRF defence; the PKCE
verifier was returned to the browser, which is the secret PKCE protects; and no
ID token was ever verified, so the committed `verify_id_token` was dead code.

The `DELETE ... RETURNING` matters specifically: a token that merely persisted
would let a second callback replay it, so a stolen login URL would not be
single-use.

An unlinked identity may not create an account, because with open registration
that is indistinguishable from self-service signup — and an enterprise
deployment should be inviting users, not accepting whatever claims arrive. The
email must match the *verified* claim exactly; a matching address is a claim to
be confirmed by the identity link, never proof by itself.

The integration test that previously certified the bypass as correct is
replaced with tests of these properties, because a test asserting that an
unauthenticated POST returns a session is worse than no test: it tells the next
reader that SSO is verified.

## ADR-0029 SSO identity providers are global; service accounts are tenant-scoped

Status: Accepted.

Amends: 11-security.md §11.4.

Decision:
`identity_providers` has no `tenant_id` and is administered with
`users:write`. `service_accounts` is tenant-scoped, protected by the same
row-level-security policy as every other tenant table, and administered with
`users:write` inside its own tenant.

Reason:
An identity provider is a property of the *deployment*, not of a tenant: the
same Okta tenant signs in to several Forge tenants, and making the row
tenant-scoped would mean registering the same issuer once per tenant and
leaving the uniqueness of `issuer` unenforced across them.

That is exactly why provider administration needs a stronger gate than a
tenant-scoped permission, and why the audit trail on it matters more than for an
ordinary CRUD endpoint — deleting the deployment's SSO provider locks every
tenant out at once.

Service accounts are the opposite: they belong to one tenant's automation and
must never be reachable from another.

Migration 023 closes the gap migration 021 opened — `service_accounts` was
created after migration 020 had already built its policy list, so it shipped
tenant-scoped and unprotected, and the isolation guard test did not name it.

## ADR-0030 A job key is unique per environment, not per tenant

Status: Accepted.

Amends: 02-domain-model.md §2.2 (`key` uniqueness); 08-storage-specification.md §8.

Decision:
`jobs.key` is unique across `(tenant_id, environment_id, key)`. Rows with a null
`environment_id` keep one-per-tenant uniqueness through a second partial index.

Reason:
`key` was unique across `(tenant_id, key)`, which makes cross-environment
migration impossible in a way that is not obvious until it is attempted. The dev
copy and the prod copy of one job share a key *by definition* — the key is what
a migration matches on — so creating the second one failed with a unique
violation. The feature could not work at all inside a single tenant.

Scoping to the environment keeps the property that makes the key useful: a deep
link carrying `?environment=prod&key=nightly` still resolves to exactly one job.

The null case is handled rather than ignored. A job created before environments
existed has no environment, and forcing every legacy row into one sentinel
environment would collide them all; the second partial index gives those rows the
old semantics so an operator who has not yet assigned an environment still cannot
create two jobs with one key.

## Future ADR candidates

- Queue implementation.
- Authentication mechanism.
- Secret backend abstraction.
- Executor sandbox.
- Broker adapter.
- Multi-region model.
- Workflow language.
- WASM executor.
- Event trigger model.
