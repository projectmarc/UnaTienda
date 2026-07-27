# Sales and Payments Specification

## Purpose

Define sale confirmation, optional descriptive data, tender methods, split payments, and annulment traceability.

## Requirements

### Requirement: Confirm a sale by total

The system MUST require only a positive sale total as sale content and MAY accept optional details and notes. It MUST create no posted sale until confirmation succeeds.

#### Scenario: Confirm a minimal sale

- GIVEN an open session and a positive total
- WHEN the operator confirms without details or notes
- THEN the system records the sale with those fields empty

#### Scenario: Reject an invalid total

- GIVEN an open session
- WHEN the operator submits a zero, negative, or non-exact total
- THEN the system rejects the sale without changing session totals

### Requirement: Support split tenders and optional transfer accounts

The system MUST support cash, transfer, debit, and credit payments. It MUST allow one or more payment entries whose exact sum equals the sale total. A transfer entry MAY identify an account; an account MUST NOT be required for confirmation. Sales MUST be internal records and MUST NOT require a customer receipt.

#### Scenario: Confirm a split payment

- GIVEN a sale total and cash, transfer, debit, or credit entries
- WHEN their sum exactly equals the total
- THEN the system posts each tender with its method and amount

#### Scenario: Reject an incomplete split

- GIVEN payment entries sum to less or more than the sale total
- WHEN confirmation is requested
- THEN the system rejects the sale and posts no tender

### Requirement: Annul a confirmed sale

The system MUST retain a confirmed sale after annulment, mark its status, and record a linked reversal of its posted payment effects. A reason MAY be supplied but MUST NOT be required. Annulment MUST obey session finality.

#### Scenario: Annul while the session is open

- GIVEN a confirmed sale in the open session
- WHEN the operator confirms annulment without a reason
- THEN the sale remains traceable as annulled and its payment effects are reversed

#### Scenario: Reject annulment after closure

- GIVEN the sale belongs to a closed session
- WHEN annulment is requested
- THEN the system rejects it without changing the closed record
