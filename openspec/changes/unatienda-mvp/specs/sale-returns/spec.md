# Sale Returns Specification

## Purpose

Define traceable total and partial returns linked to confirmed sales, with cumulative limits and payment reversals.

## Requirements

### Requirement: Link and limit returns

The system MUST link every return to a confirmed sale and MUST record whether it is total or partial. The cumulative returned amount MUST be positive and MUST NOT exceed the original sale total. Returns MUST be posted in an open session, including when the original sale is historical.

#### Scenario: Record a partial then total return

- GIVEN a confirmed sale with an unreversed balance
- WHEN a partial return is posted and later the remaining balance is returned
- THEN both returns remain linked and cumulative returns equal the sale total

#### Scenario: Reject an excessive cumulative return

- GIVEN prior returns already consume the sale's remaining balance
- WHEN another return exceeds that balance
- THEN the system rejects it without posting a return or reversal

### Requirement: Reverse the original payment methods

The system MUST record payment reversals for the original tender methods. For a partial return, each method's reversal MUST NOT exceed its unreversed original amount, and all reversal amounts MUST sum exactly to the return amount. A total return MUST reverse every remaining original tender balance.

#### Scenario: Reverse a split-tender return

- GIVEN a sale paid with multiple methods
- WHEN a valid return allocation is confirmed
- THEN method-specific reversals are recorded and sum exactly to the return

#### Scenario: Reject an untraceable reversal

- GIVEN a proposed reversal exceeds an original method balance or does not sum to the return
- WHEN the return is confirmed
- THEN the system rejects it without altering payment balances
