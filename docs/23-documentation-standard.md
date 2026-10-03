# 23. Documentation Standard

## 23.1 Every feature must specify

1. Purpose.
2. Scope.
3. Actors.
4. Inputs.
5. Outputs.
6. State changes.
7. Validation.
8. Authorization.
9. Failure behavior.
10. Retry behavior.
11. Concurrency behavior.
12. Persistence.
13. Observability.
14. Audit behavior.
15. API behavior.
16. UI behavior.
17. CLI behavior where applicable.
18. Security impact.
19. Migration impact.
20. Testing requirements.
21. Acceptance criteria.

## 23.2 No undocumented behavior

If implementation behavior is user-visible or affects execution correctness, it must be documented.

## 23.3 Public API documentation

Every public API:
- has rustdoc;
- has an example where practical;
- documents errors;
- documents panics if any;
- documents concurrency/thread-safety semantics if relevant.

## 23.4 Specification changes

A specification change requires:
- updated affected documents;
- acceptance test changes;
- ADR/RFC where required;
- changelog entry for released behavior.

## 23.5 Examples

Examples must be runnable or explicitly marked illustrative.

## 23.6 Generated API documentation

OpenAPI and rustdoc are generated from source but MUST be reviewed for completeness.

## 23.7 Documentation CI

CI SHOULD fail if:
- broken links;
- missing public docs;
- OpenAPI generation differs from committed spec;
- examples fail;
- schema validation fails.
