# 11. Security

## 11.1 Security principles

- Least privilege.
- Secure defaults.
- Defense in depth.
- Explicit tenant isolation.
- No secrets in logs.
- No implicit trust of workers.
- No unauthenticated mutation endpoints.
- Fail closed.

## 11.2 Threat model

Protect against:
- unauthorized API access;
- cross-tenant data access;
- stolen API keys;
- malicious job definitions;
- malicious worker registration;
- command injection;
- SSRF;
- secret exfiltration;
- log injection;
- dependency vulnerabilities;
- replayed requests;
- stale worker completion;
- queue poisoning;
- denial of service;
- privilege escalation.

## 11.3 Authentication

V1:
- local session authentication (Email/Password) for web UI;
- SSO/OIDC integration (Google, GitHub, Okta, SAML 2.0) for enterprise;
- multi-factor authentication (TOTP, WebAuthn);
- API keys/service tokens for automation;
- worker credentials.

Passwords MUST be hashed using an approved password hashing algorithm (e.g., Argon2, bcrypt).

API tokens MUST be stored hashed using SHA-256 or stronger.

Session management MUST use secure HttpOnly cookies for web UI, and JWT tokens with rotating refresh tokens for APIs.

## 11.4 Authorization

Use RBAC with resource scope.

Permission format concept:

`resource:action`

Examples:
- jobs:read
- jobs:write
- jobs:trigger
- executions:read
- executions:cancel
- workers:admin
- audit:read

## 11.5 Tenant isolation

Every authorization path MUST validate tenant scope.

Never rely solely on UI restrictions.

## 11.6 Job execution security

User-supplied commands are dangerous.

Container/process executors MUST support:
- restricted filesystem;
- environment allowlist;
- resource limits;
- network policy;
- non-root execution where possible;
- capability dropping;
- read-only filesystem where practical;
- timeout;
- output limits.

## 11.7 SSRF

HTTP executor MUST protect against:
- localhost access;
- link-local addresses;
- cloud metadata endpoints;
- private network ranges;
- DNS rebinding.

Allowlist-based outbound access SHOULD be available.

## 11.8 Secrets

Secrets MUST be referenced by identifier.

UI/API responses MUST never return secret values after creation unless the specific secret-management backend explicitly requires one-time display.

Logs MUST redact:
- authorization headers;
- cookies;
- API keys;
- passwords;
- tokens;
- configured sensitive fields.

## 11.9 API abuse

Implement:
- request size limits;
- rate limits;
- concurrency limits;
- pagination caps;
- log size caps;
- execution creation limits.

## 11.10 Audit

Security-sensitive actions MUST be audited.

Audit logs MUST be append-only from normal application code.

## 11.11 Dependencies

CI MUST run dependency vulnerability/license checks.

Recommended Rust ecosystem controls:
- `cargo audit`
- `cargo deny`
- lockfile review
- Dependabot or equivalent

RustSec maintains a public advisory database used by these tools. urlRustSec Advisory Databasehttps://github.com/RustSec/advisory-db

## 11.12 Supply chain

CI SHOULD:
- pin actions by commit where practical;
- restrict permissions;
- generate SBOM;
- sign release artifacts;
- publish checksums;
- use reproducible builds where practical.

## 11.13 Security disclosure

See `18-open-source-governance.md`.

Do not ask public issue reporters to disclose exploitable vulnerabilities publicly.
