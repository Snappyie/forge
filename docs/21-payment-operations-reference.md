# 21. Payment Operations Reference Workload

This document defines a safe synthetic workload for demonstrating Forge in a payment-systems context.

Forge does not process real funds.

## 21.1 Synthetic transaction lifecycle

```text
CREATED
  ↓
AUTHORIZED
  ↓
CAPTURED
  ↓
SETTLED
```

Alternative:
```text
AUTHORIZED → FAILED
CAPTURED → REFUND_PENDING → REFUNDED
```

## 21.2 Daily settlement workflow

```text
Fetch transactions
        ↓
Validate
        ↓
Reconcile
      /   \
     /     \
Fee calc   Fraud checks
     \     /
      \   /
    Settlement
        ↓
Generate report
        ↓
Notify
```

## 21.3 Reconciliation

Inputs:
- internal synthetic transactions;
- synthetic provider transactions.

Outputs:
- matched;
- missing internally;
- missing externally;
- amount mismatch;
- currency mismatch;
- duplicate;
- status mismatch.

## 21.4 Idempotency exercise

Send the same settlement command twice.

Expected:
- one logical settlement operation;
- second request resolves to existing operation/result or conflict according to key semantics.

## 21.5 Failure exercise

Simulate:
- provider timeout;
- provider 500;
- malformed response;
- network disconnect;
- worker crash after external side effect.

The last case MUST demonstrate why idempotency is necessary.

## 21.6 Audit exercise

Show:
- who triggered settlement;
- which version ran;
- which worker ran it;
- which attempts occurred;
- which retry policy was applied;
- final result.

## 21.7 Performance exercise

Generate synthetic transactions and benchmark:
- parsing;
- reconciliation;
- aggregation;
- persistence;
- workflow scheduling.

No real card numbers, bank credentials, PANs, CVVs, or production identifiers are permitted in examples.
