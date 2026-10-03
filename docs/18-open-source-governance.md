# 18. Open Source Governance

## 18.1 License

Recommended dual license:
- MIT
- Apache-2.0

Every repository component must have compatible licensing.

Third-party licenses MUST be tracked.

## 18.2 Required repository files

- README.md
- LICENSE-MIT
- LICENSE-APACHE
- CONTRIBUTING.md
- CODE_OF_CONDUCT.md
- SECURITY.md
- GOVERNANCE.md
- CHANGELOG.md
- SUPPORT.md
- .gitignore
- rust-toolchain.toml
- Cargo.toml
- Cargo.lock where applicable
- CI workflow definitions

## 18.3 Contribution process

Every contribution:
1. Must pass CI.
2. Must include tests for behavior changes.
3. Must update documentation for externally visible behavior.
4. Must not introduce undocumented breaking behavior.
5. Must pass security/license checks.
6. Must include migration notes when schema changes.

## 18.4 Commit convention

Use conventional commits or another documented convention.

Recommended:
- feat
- fix
- docs
- refactor
- test
- perf
- build
- ci
- security
- chore

## 18.5 Pull request requirements

PR must contain:
- problem;
- solution;
- alternatives considered;
- testing;
- operational impact;
- security impact;
- migration impact;
- documentation changes.

## 18.6 RFC process

Architecture or user-visible changes that cannot be safely reviewed as a normal PR require an RFC.

Use `docs/templates/rfc-template.md`.

## 18.7 ADR process

Important architectural choices require ADRs.

Use `docs/templates/adr-template.md`.

## 18.8 Security disclosure

Private reporting channel must be published in SECURITY.md.

Security reports MUST NOT be handled solely through public GitHub issues.

Security fixes SHOULD include:
- advisory;
- affected versions;
- fixed versions;
- mitigation;
- test coverage.

## 18.9 Release policy

Use semantic versioning for published libraries/APIs.

Release checklist:
- CI green;
- security audit;
- license audit;
- migration test;
- upgrade test;
- rollback documentation;
- changelog;
- release notes;
- container image;
- checksums;
- SBOM;
- signed artifacts where infrastructure supports it.

Cargo feature additions and dependency changes can affect compatibility; Cargo's documentation explicitly describes feature and SemVer considerations. urlCargo feature compatibility guidancehttps://doc.rust-lang.org/cargo/reference/features.html

## 18.10 Deprecation

Deprecations require:
- notice;
- replacement;
- timeline;
- migration instructions;
- removal version.

## 18.11 Community

Provide:
- Discussions for design questions;
- Issues for actionable bugs/features;
- security channel for vulnerabilities;
- roadmap;
- release notes.

## 18.12 Maintainer rules

At least two maintainers SHOULD be able to perform releases.

No single-person dependency should exist for:
- signing;
- publishing;
- deployment;
- security response.
