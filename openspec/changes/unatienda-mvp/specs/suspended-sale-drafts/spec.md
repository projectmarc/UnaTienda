# Suspended Sale Drafts Specification

## Purpose

Define non-posting sale drafts that can be paused during one active session without contaminating cash or reports.

## Requirements

### Requirement: Preserve multiple active-session drafts

The system MUST allow multiple named or identifiable drafts in the open session. A draft MUST NOT affect cash, reconciliation, session totals, or historical posted transactions until confirmed as a sale.

#### Scenario: Suspend and resume a draft

- GIVEN an open session and an unconfirmed sale
- WHEN the operator suspends it and later resumes it
- THEN its entered total, details, notes, and payment entries are restored without posting

#### Scenario: Keep drafts independent

- GIVEN multiple drafts exist
- WHEN one draft is resumed or discarded
- THEN the other drafts remain unchanged and available

### Requirement: Enforce draft lifecycle boundaries

The system MUST allow a draft to be discarded without posting it. Drafts MUST NOT be resumed in another session and MUST prevent closing while any draft remains.

#### Scenario: Block closing with drafts

- GIVEN at least one suspended draft exists
- WHEN the operator attempts to close the session
- THEN the system blocks closure and identifies that drafts must be resumed or discarded

#### Scenario: Reject cross-session resume

- GIVEN a session was closed while no draft existed, or a new session is open
- WHEN a draft from another session is resumed
- THEN the system rejects the operation and creates no sale
