# Design: UnaTienda MVP

## Technical Approach

Build a Tauri monolith: React drives workflows; typed Tauri commands call Rust application services; domain types enforce rules; repositories execute atomic SQLite transactions. SQLite is the operational source of truth.

## Architecture Decisions

| Decision | Choice | Alternative / tradeoff | Rationale |
|---|---|---|---|
| Boundaries | `commands → application → domain ← persistence`; React never computes totals | Frontend logic is quicker | Single enforcement point for offline integrity. |
| Money | Rust `Money(i64)` / SQLite `INTEGER` in centavos, IPC as decimal strings | JS numbers / SQL REAL | Exact sums; rejects excess precision and overflow. |
| Records | Append reversals/returns; only `sale.status` may become `annulled` | Delete/update history | Preserves traceability; closure snapshots are final. |
| Concurrency | One SQLite pool, `BEGIN IMMEDIATE` per mutation | Optimistic multi-writer | Serializes check-and-write invariants on one PC. |
| UI | Feature folders with route-level workflows | Generic dashboard | Mirrors operator tasks; state reloads after mutations. |

## Domain, State, and Data Flow

`CashSession: Open → Closed`; `Sale: Confirmed → Annulled`; `Draft: Suspended → Confirmed|Discarded`; returns and movements are immutable. A historical sale may be returned only into the currently open session; annulment is allowed only in the sale's still-open session.

A sale and its payments are owned by the session in which the sale was confirmed; after closure the sale is immutable. A return is a *current-session* transaction that references a historical `sale_id`. Its `payment_reversals` are new rows linked to the original `payment_id` and the new `return_id`, reducing expected cash in the current session without mutating the historical sale.

Expected cash = opening + cash payments − cash reversals + income − expense − withdrawal. Non-cash tenders never affect expected cash.

## Database and Transaction Rules

Migrations create `cash_sessions`, `sales`, `payments`, `drafts`, `movements`, `returns`, `payment_reversals`, `idempotency_keys`, `schema_migrations`, and `backup_runs`. Foreign keys enabled; UTC timestamps; UUIDs; reversal source XOR constraint; partial unique index enforcing one open session.

Sale confirmation inserts sale/payments and deletes an optional source draft atomically. Annulment changes status and inserts a reversal row per unreversed payment balance. Return creation verifies cumulative limits and exact allocation, then inserts a current-session return/reversals. Closure verifies open status and zero drafts, derives expected cash, stores snapshots, and closes. Triggers and guards reject alteration of closed-session-owned rows.

## Idempotency and Atomic Mutation Commit

Every financial mutation carries a client `idempotency_key`. The `idempotency_keys` table defines `idempotency_key TEXT PRIMARY KEY` and stores `command`, `request_hash`, and `response_json` for committed mutations.

A mutation runs inside one `BEGIN IMMEDIATE` transaction:

1. Look up the key. A matching `request_hash` returns the stored response; a different hash returns `IDEMPOTENCY_KEY_REUSE` (non-retryable).
2. If absent, execute the financial mutation and insert its key plus canonical `response_json` in the same transaction.
3. Commit both together. `BEGIN IMMEDIATE` serializes concurrent writers, so a later writer observes either the committed key or no key.

If the transaction rolls back, neither the financial mutation nor the idempotency row exists, so retry is safe. If commit succeeds but the IPC response is lost, retry finds the persisted unique key and replays the committed response.

`get_mutation_status(key)` returns `committed` plus the stored response, or `not_found`. There are no durable reservation, pending, owner, lease, timeout, or orphan-cleanup states.

## Tauri Contracts and Errors

Commands cover state/reconciliation, session open/close, draft CRUD, sale confirm/annul, movement record, return create, history detail, backup export/restore/validate, and `get_mutation_status`. Mutation requests carry money strings, IDs, and an `idempotency_key`; responses return canonical money strings and refreshed views.

`create_return` fields:

| Field | Type | Rule |
|---|---|---|
| `sale_id` | UUID | Historical confirmed sale. |
| `idempotency_key` | UUID | Client mutation key. |
| `return_kind` | `total` \| `partial` | Total reverses all remaining balances. |
| `amount_minor` | u64 | Partial only. |
| `allocation` | `[{payment_id, amount_minor}]` | Partial sums exactly; total covers all remaining balances. |

The use case verifies cumulative returns ≤ sale total and each line ≤ the payment's remaining unreversed balance, then inserts a `returns` row owned by the current session plus one `payment_reversals` row per allocation.

Errors use `{code, message, field?, retryable}` with codes `VALIDATION`, `NO_OPEN_SESSION`, `SESSION_CLOSED`, `DRAFTS_BLOCK_CLOSE`, `CONFLICT`, `IDEMPOTENCY_KEY_REUSE`, `IO`, `INCOMPATIBLE_BACKUP`.

## Backup, Restore, Migrations, and Security

Enable WAL, foreign keys, `busy_timeout=5000`, `synchronous=FULL`, `journal_mode=WAL`. On startup run migrations inside transactions after a pre-migration online-backup snapshot.

Daily backups: a background task runs on startup and every 15 minutes while the app is open. It checks `backup_runs` for a successful run on the current calendar day and, if missing, creates an online backup into a temp file, runs `integrity_check`, atomically renames it, and records success. Failures are surfaced and never alter the live DB.

WAL/SHM: the live DB is a trio. Use SQLite's online backup API so the backup is self-contained; never copy the three files separately.

Restore uses a durable swap journal (`restore_swap.json`) and an explicit phase/state machine: `staged → pre_swap → post_swap → verified → (rollback)`. The journal is written, fsynced, and closed before any destructive step.

| Phase | Action |
|---|---|
| `staged` | Validated backup copied to a staging path; journal records `staged_path`, `live_path`, `snapshot_path`, checksums, and phase. |
| `pre_swap` | Live DB trio snapshotted to `*.pre-restore.{timestamp}.*`; journal updated and fsynced. |
| `post_swap` | Staged backup atomically renamed over the live path; old WAL/SHM removed; journal updated and fsynced. |
| `verified` | Database reopened; `integrity_check` and invariants pass; journal updated, then journal and snapshot deleted with directory fsync. |
| `rollback` | On verification failure or cancel, snapshot restored over live path, staged file removed, journal marked `rollback` then deleted. |

Startup recovery reads the journal phase deterministically, not target-file presence:

| Phase | Recovery |
|---|---|
| `staged` | Roll back: remove staged file and journal (live trio untouched). |
| `pre_swap` | Roll back to snapshot, remove staged file and journal. |
| `post_swap` | Reopen and verify; pass → finish cleanup, fail → roll back to snapshot. |
| `verified` / `rollback` | Remove journal and any leftover snapshot/staged files. |

Directory durability: after every journal write, file creation, deletion, or rename, fsync the parent directory. Atomic rename replaces the live DB file. This guarantees deterministic recovery without relying on whether the target file exists.

Export copies a verified backup to a user-selected path. File dialogs constrain paths; commands expose no arbitrary filesystem access; CSP is restrictive; logs exclude notes/account details.

## File Changes

| Path | Action | Purpose |
|---|---|---|
| `src-tauri/src/{domain,application,persistence,commands}/` | Create | Rules, use cases, SQLite adapters, IPC |
| `src-tauri/migrations/`, `src-tauri/src/backup/` | Create | Schema, triggers, migration/backup safety |
| `src/features/`, `src/shared/tauri/`, `src/app/` | Create | Workflows, typed adapter, routing/state |
| `.github/workflows/windows.yml` | Create | Test and build unsigned Windows installer |

## Testing Strategy

Unit-test money, transitions, reconciliation, return allocation, idempotency collision, and UI reducers. Integration-test commands against temporary SQLite, rollback, closure triggers, migrations, crash/reopen, idempotency replay, and backup/restore failure including swap-journal recovery. React tests use a fake command adapter; Windows CI runs both suites and an installer smoke build.

## Migration / Rollout

Initial schema only; later builds apply monotonic migrations with mandatory backup. Unsigned installer is CI-built; SmartScreen limitation documented. No feature flags or cloud migration.

## Open Questions

None.
