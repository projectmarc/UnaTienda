# Session History Specification

## Purpose

Define offline browsing of session history and detailed internal transaction records.

## Requirements

### Requirement: Browse sessions and transaction details

The system MUST allow browsing open and closed sessions and MUST show each session's opening amount, posted sales, annulments, returns, movements, counted cash, expected cash, and closing difference. Sale details MUST include total, status, payment breakdown, links to returns or reversals, and optional details or notes.

#### Scenario: Inspect a closed session

- GIVEN a closed session with sales, returns, and movements
- WHEN the operator opens its history detail
- THEN the system shows the complete traceable session and transaction totals

#### Scenario: Inspect an internal record

- GIVEN a movement or annulment exists
- WHEN the operator views its detail
- THEN the system identifies its type, amount, session, status, and related records without requiring a receipt

### Requirement: Preserve historical truth

The system MUST retain annulled sales, returns, and reconciliation differences in history. Historical views MUST reflect immutable posted records and MUST NOT present drafts as confirmed transactions.

#### Scenario: Distinguish draft from posted sale

- GIVEN a session contains a suspended draft and a confirmed sale
- WHEN history is viewed
- THEN only the confirmed sale appears in posted totals and the draft is identified as non-posting

#### Scenario: Show linked return history

- GIVEN a sale has one or more returns
- WHEN the sale detail is viewed
- THEN each return, cumulative amount, and payment reversal remains navigable and traceable
