# Local Backup Specification

## Purpose

Define resilient local protection and export of the operational record without a cloud dependency.

## Requirements

### Requirement: Create automatic daily backups

The system MUST attempt at least one automatic backup per calendar day when the application is available and MUST preserve a complete, restorable local record. Backup creation MUST NOT partially replace the active record.

#### Scenario: Complete the daily backup

- GIVEN the application is available and the day has no successful backup
- WHEN the daily backup operation runs
- THEN a restorable backup of sessions and transaction history is recorded

#### Scenario: Recover from a backup failure

- GIVEN the automatic backup cannot be completed
- WHEN the failure is detected
- THEN the current operational data remains usable, the failure is surfaced, and a later retry remains possible

### Requirement: Export and restore manually

The system MUST provide a manual export of the complete local operational record to a user-selected destination. An exported backup MUST be restorable as a coherent record; a failed restore MUST NOT destroy the current record.

#### Scenario: Export and restore

- GIVEN sessions and linked transactions exist
- WHEN the operator exports and restores the backup
- THEN the restored record retains monetary values, links, statuses, and session finality

#### Scenario: Reject an unusable backup safely

- GIVEN an export or backup is missing, corrupt, or incompatible
- WHEN restore is requested
- THEN the system reports the error and preserves the current operational data
