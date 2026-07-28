# Tasks: UnaTienda MVP

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 2400–3400 |
| 400-line budget risk | High |
| Chained PRs recommended | Yes |
| Suggested split | PR 1 → PR 2 → PR 3 → PR 4 → PR 5 → PR 6 → PR 7 |
| Delivery strategy | ask-on-risk |
| Chain strategy | stacked-to-main |

Decision needed before apply: Yes
Chained PRs recommended: Yes
Chain strategy: stacked-to-main
400-line budget risk: High

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | Project scaffold + SQLite schema + connection + idempotency foundation | PR 1 | Tauri 2 init, migrations, DB pool, Money type, idempotency key model. ~350 lines. |
| 2 | Cash session domain + commands + session management UI | PR 2 | Open/close/reconcile. Rust domain, persistence, commands, React session view. ~450 lines. |
| 3 | Sales & payments domain + commands + sales UI | PR 3 | Confirm/annul sales, split tenders. Depends on PR 2. ~400 lines. |
| 4 | Suspended drafts domain + commands + drafts UI | PR 4 | Draft CRUD, lifecycle guards. Depends on PR 3 (draft delete on confirm). ~250 lines. |
| 5 | Cash movements + sale returns domain + commands + UI | PR 5 | Income/expense/withdrawal + return allocation. Depends on PR 3. ~450 lines. |
| 6 | Session history + local backup domain + commands + UI + restore journal | PR 6 | History browsing, daily backup, restore state machine. Depends on PR 2. ~500 lines. |
| 7 | Windows CI + integration tests + polish | PR 7 | GitHub Actions workflow, integration tests for backup/restore, idempotency. ~300 lines. |

## Phase 1: Project Scaffold & Foundation

- [x] 1.1 Initialize Tauri 2 project: `cargo tauri init` with React/Vite TypeScript template; verify `src-tauri/`, `src/`, `package.json`, `vite.config.ts` scaffold builds
- [x] 1.2 Create `src-tauri/src/domain/money.rs`: `Money(i64)` type with `from_cents`, `add`, `sub`, `checked_add`, `to_string` (decimal IPC), `FromStr` parsing with precision rejection
- [x] 1.3 Create `src-tauri/src/domain/error.rs`: App error enum (`VALIDATION`, `NO_OPEN_SESSION`, `SESSION_CLOSED`, `DRAFTS_BLOCK_CLOSE`, `CONFLICT`, `IDEMPOTENCY_KEY_REUSE`, `IO`, `INCOMPATIBLE_BACKUP`) with `{code, message, field?, retryable}` JSON serialization
- [x] 1.4 Create `src-tauri/migrations/001_initial.sql`: schema with `cash_sessions`, `sales`, `payments`, `drafts`, `movements`, `returns`, `payment_reversals`, `idempotency_keys`, `schema_migrations`, `backup_runs` tables; foreign keys, UUIDs, UTC timestamps, partial unique index for one open session, reversal source XOR constraint
- [x] 1.5 Create `src-tauri/src/persistence/connection.rs`: SQLite pool with WAL, `foreign_keys=ON`, `busy_timeout=5000`, `synchronous=FULL`, connection initialization, migration runner inside transactions with pre-migration backup snapshot
- [x] 1.6 Create `src-tauri/src/persistence/idempotency.rs`: `IdempotencyKey` lookup/insert/commit in same transaction, `get_mutation_status` returning committed response or not_found
- [x] 1.7 Write unit tests for `Money` arithmetic, overflow rejection, IPC string conversion, and `FromStr` parsing

## Phase 2: Cash Session Domain

- [x] 2.1 Create `src-tauri/src/domain/cash_session.rs`: `CashSession` struct, `open` / `close` / `reconcile` methods enforcing single-open-session invariant, zero-drafts closure guard, expected cash formula (`opening + cash_payments - cash_reversals + income - expense - withdrawal`)
- [x] 2.2 Create `src-tauri/src/persistence/cash_session_repo.rs`: insert open session (with unique index guard), query current open session, snapshot and close atomically, query expected cash aggregations
- [x] 2.3 Create `src-tauri/src/application/cash_session_use_case.rs`: `OpenSession`, `CloseSession`, `ReconcileSession` use cases wrapping domain + persistence with idempotency key
- [x] 2.4 Create `src-tauri/src/commands/cash_session_commands.rs`: `open_session`, `close_session`, `get_reconciliation` Tauri commands returning canonical money strings and refreshed views
- [x] 2.5 Create `src/features/sessions/SessionView.tsx`: display open session with opening amount, expected cash, counted cash input, difference, close button; route-level workflow
- [x] 2.6 Write tests: open session, reject second session, close with difference, reject post-close mutation, expected cash calculation correctness

## Phase 3: Sales & Payments

- [ ] 3.1 Create `src-tauri/src/domain/sale.rs`: `Sale` struct (confirmed/annulled status), `PaymentEntry` with method enum (cash/transfer/debit/credit), `confirm` enforcing positive total and exact tender sum, `annul` inserting reversal rows per unreversed payment balance
- [ ] 3.2 Create `src-tauri/src/persistence/sale_repo.rs`: insert sale + payments atomically (delete source draft if present), update status on annul, insert reversal rows, query sale with payments and return links
- [ ] 3.3 Create `src-tauri/src/application/sale_use_case.rs`: `ConfirmSale`, `AnnulSale` use cases with idempotency key, session-open guard, annulment-only-in-own-session guard
- [ ] 3.4 Create `src-tauri/src/commands/sale_commands.rs`: `confirm_sale`, `annul_sale` Tauri commands with money string I/O
- [ ] 3.5 Create `src/features/sales/SaleWorkflow.tsx`: sale entry with total input, optional details/notes, payment method selection, split-tender list, confirm/annul buttons; state reloads after mutation
- [ ] 3.6 Write tests: confirm minimal sale, reject zero/negative total, confirm split payment, reject incomplete split, annul while open, reject annul after closure

## Phase 4: Suspended Drafts

- [ ] 4.1 Create `src-tauri/src/domain/draft.rs`: `Draft` struct (suspended state), `suspend` / `resume` / `discard` methods; drafts must not affect cash or posted totals
- [ ] 4.2 Create `src-tauri/src/persistence/draft_repo.rs`: insert/suspend, resume, discard, list active drafts, count drafts (for closure guard)
- [ ] 4.3 Create `src-tauri/src/application/draft_use_case.rs`: `SuspendDraft`, `ResumeDraft`, `DiscardDraft` use cases; cross-session resume rejection
- [ ] 4.4 Create `src-tauri/src/commands/draft_commands.rs`: `suspend_draft`, `resume_draft`, `discard_draft`, `list_drafts` Tauri commands
- [ ] 4.5 Create `src/features/drafts/DraftsList.tsx`: list suspended drafts, resume/discard buttons, draft count badge near close button
- [ ] 4.6 Write tests: suspend/resume preserves data, discard without posting, block closing with drafts, reject cross-session resume

## Phase 5: Cash Movements & Sale Returns

- [ ] 5.1 Create `src-tauri/src/domain/movement.rs`: `Movement` struct (income/expense/withdrawal), validate positive amount, session-open guard, immutable after insert
- [ ] 5.2 Create `src-tauri/src/domain/return_domain.rs`: `SaleReturn` struct (total/partial), `create` verifying cumulative returned ≤ sale total, per-payment remaining balance, exact allocation sum; inserts return + payment_reversals rows owned by current session
- [ ] 5.3 Create `src-tauri/src/persistence/movement_repo.rs` and `return_repo.rs`: insert movements, insert return + reversals atomically, query returnable balance per payment
- [ ] 5.4 Create `src-tauri/src/application/movement_use_case.rs` and `return_use_case.rs`: use cases with idempotency keys
- [ ] 5.5 Create `src-tauri/src/commands/movement_commands.rs` and `return_commands.rs`: `record_movement`, `create_return` Tauri commands
- [ ] 5.6 Create `src/features/movements/MovementForm.tsx` and `src/features/returns/ReturnForm.tsx`: UI for recording income/expense/withdrawal and creating total/partial returns with per-payment allocation
- [ ] 5.7 Write tests: record income/expense/withdrawal, reject zero/negative amount, reject movement on closed session, partial then total return, reject excessive return, reject untraceable reversal

## Phase 6: Session History & Local Backup

- [ ] 6.1 Create `src-tauri/src/persistence/history_repo.rs`: query session list (open + closed), session detail with sales/annulments/returns/movements, sale detail with payment breakdown and linked returns
- [ ] 6.2 Create `src-tauri/src/commands/history_commands.rs`: `list_sessions`, `get_session_detail`, `get_sale_detail` Tauri commands
- [ ] 6.3 Create `src-tauri/src/backup/daily_backup.rs`: background task on startup + every 15 min, check `backup_runs` for current day, online backup → integrity_check → atomic rename → record success; failures surfaced, never alter live DB
- [ ] 6.4 Create `src-tauri/src/backup/restore.rs`: durable swap journal (`restore_swap.json`) state machine: staged → pre_swap → post_swap → verified → rollback; deterministic startup recovery per journal phase; directory fsync after every mutation
- [ ] 6.5 Create `src-tauri/src/backup/export.rs`: copy verified backup to user-selected path via file dialog
- [ ] 6.6 Create `src-tauri/src/commands/backup_commands.rs`: `export_backup`, `restore_backup`, `validate_backup` Tauri commands
- [ ] 6.7 Create `src/features/history/SessionHistory.tsx` and `src/features/backup/BackupPanel.tsx`: history browsing UI and backup export/restore triggers
- [ ] 6.8 Write tests: session list/detail correctness, history excludes drafts, daily backup creation + failure surfacing, restore swap journal state machine, restore failure preserves current data, export round-trip

## Phase 7: CI, Integration Tests & Polish

- [ ] 7.1 Create `.github/workflows/windows.yml`: cargo test, vitest, build unsigned Windows installer; cache Rust/Node deps
- [ ] 7.2 Create `src-tauri/tests/integration/`: test commands against temporary SQLite — open→sale→annul→return→close lifecycle, closure trigger blocking, migration replay, crash/reopen recovery, idempotency replay
- [ ] 7.3 Create `src-tauri/src/commands/mod.rs` and `src-tauri/src/lib.rs`: wire all commands into Tauri `generate_handler!`, register migration + backup startup hooks
- [ ] 7.4 Create `src/app/App.tsx` and `src/app/router.tsx`: feature-folder routing, state reload after mutations, layout with session context provider
- [ ] 7.5 Write end-to-end test: full session lifecycle from open through sales, returns, movements, to close, verifying expected cash and immutability
- [ ] 7.6 Update `README.md` with build instructions, SmartScreen warning documentation, and architecture overview
