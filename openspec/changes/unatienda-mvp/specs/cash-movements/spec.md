# Cash Movements Specification

## Purpose

Define traceable internal income, expense, and withdrawal records that affect session reconciliation.

## Requirements

### Requirement: Record cash movements

The system MUST record positive, exact amounts classified as income, expense, or withdrawal while a session is open. Each movement MUST retain its type, amount, session link, and optional note; movements MUST be internal records and MUST NOT require a customer receipt.

#### Scenario: Record an income and expense

- GIVEN an open session
- WHEN the operator records valid income and expense amounts
- THEN both movements are posted with their classifications and affect expected cash

#### Scenario: Record a withdrawal

- GIVEN an open session and a valid amount
- WHEN the operator records a withdrawal
- THEN the withdrawal is linked to the session and reduces expected cash

### Requirement: Reject invalid or final-session movements

The system MUST reject zero or negative movement amounts and MUST reject any movement against a closed session without changing recorded totals.

#### Scenario: Reject an invalid amount

- GIVEN an open session
- WHEN a movement has a zero or negative amount
- THEN the system rejects it without posting a record

#### Scenario: Preserve closed-session finality

- GIVEN the target session is closed
- WHEN a movement is requested
- THEN the system rejects it and leaves reconciliation unchanged
