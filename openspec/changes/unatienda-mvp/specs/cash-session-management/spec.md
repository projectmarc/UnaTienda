# Cash Session Management Specification

## Purpose

Define the lifecycle, money invariants, reconciliation, and finality of the one cash session allowed on a computer.

## Requirements

### Requirement: Single active session

The system MUST allow at most one open session per computer and MUST require an opening amount before recording transactions.

#### Scenario: Open a session

- GIVEN no session is open
- WHEN the operator confirms a non-negative opening amount
- THEN one session becomes open with that amount recorded

#### Scenario: Reject a second session

- GIVEN a session is already open
- WHEN the operator attempts to open another session
- THEN the system rejects the operation and preserves the existing session

### Requirement: Monetary invariants

The system MUST represent monetary values exactly in the smallest supported currency unit without rounding. A confirmed sale's tender total MUST equal its sale total; session expected cash MUST equal opening cash plus cash-affecting sales and movements minus cash-affecting returns and reversals.

#### Scenario: Reject an unbalanced sale

- GIVEN a sale total and payment entries whose sum differs from that total
- WHEN confirmation is requested
- THEN the system rejects the sale without posting any payment

#### Scenario: Calculate expected cash

- GIVEN an open session with sales, returns, and movements
- WHEN reconciliation is requested
- THEN the system reports the exact expected cash and its difference from counted cash

### Requirement: Reconcile and close immutably

The system MUST permit closing only when no suspended drafts remain. It MUST record counted cash and any difference, then make the closed session and its posted records immutable and final.

#### Scenario: Close with an accepted difference

- GIVEN no drafts remain and counted cash differs from expected cash
- WHEN the operator confirms closure
- THEN the system records the difference and closes the session

#### Scenario: Block or reject post-close changes

- GIVEN a draft remains, or the session is already closed
- WHEN close or a mutation is requested
- THEN the system rejects the operation with no partial change
