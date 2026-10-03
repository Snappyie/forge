# 16. CLI Specification

Binary: `forge`

## 16.1 Global

```text
forge --help
forge --version
forge --config <path>
forge --context <name>
forge --output table|json|yaml
```

## 16.2 Authentication

```text
forge auth login
forge auth logout
forge auth status
forge auth contexts
```

## 16.3 Jobs

```text
forge jobs list
forge jobs get <id>
forge jobs create
forge jobs update <id>
forge jobs publish <id> --version <n>
forge jobs run <id>
forge jobs pause <id>
forge jobs resume <id>
forge jobs archive <id>
```

## 16.4 Workflows

```text
forge workflows list
forge workflows get <id>
forge workflows validate <file>
forge workflows publish <id> --version <n>
forge workflows run <id>
```

## 16.5 Executions

```text
forge executions list
forge executions get <id>
forge executions logs <id>
forge executions cancel <id>
forge executions retry <id>
forge executions dead-letter <id>
```

## 16.6 Workers

```text
forge workers list
forge workers get <id>
forge workers drain <id>
forge workers revoke <id>
```

## 16.7 Queues

```text
forge queues list
forge queues get <id>
forge queues pause <id>
forge queues resume <id>
```

## 16.8 Output

Human-readable output is default.

`--output json` MUST be machine-readable and stable.

Errors go to stderr.

Exit codes:
- 0 success
- 1 general failure
- 2 usage error
- 3 authentication failure
- 4 authorization failure
- 5 not found
- 6 conflict
- 7 validation failure
- 8 network/server failure
