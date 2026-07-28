# Review Ledger — unatienda-mvp

## Judgment Day: Technical Design — Final Scoped Re-review

**State:** JUDGMENT: ESCALATED — terminal review state preserved because the 2/2 fix budget was exhausted

Both judges verified JD-A-002 through JD-A-005. JD-A-001 is `wont-fix` in the review-process sense: the disputed reservation model was rejected by explicit user architecture decision and replaced with a single-transaction atomic idempotency model. This manual resolution does not reopen review or permit re-judgment. Warnings remain informational and non-blocking.

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| JD-A-001 | judgment-day | `openspec/changes/unatienda-mvp/design.md:31-44` | CRITICAL | wont-fix | The previously disputed durable reservation lifecycle was rejected by explicit user architecture decision. The design now commits the idempotency key, response, and financial mutation atomically; rollback leaves no key, and a lost IPC response is replayed from the committed key. | manually resolved after terminal review; no re-judgment |
| JD-A-002 | judgment-day | `openspec/changes/unatienda-mvp/design.md:67-96` | CRITICAL | verified | Both judges verified the durable phase journal, fsync ordering, deterministic phase-based startup recovery, and independence from target-file existence. | fixed and verified |
| JD-A-003 | judgment-day | `openspec/changes/unatienda-mvp/design.md:71` | CRITICAL | verified | Startup plus 15-minute current-day checks guarantee backup attempts while continuously open. | fixed and verified |
| JD-A-004 | judgment-day | `openspec/changes/unatienda-mvp/design.md:53-63` | CRITICAL | verified | The return command defines amount and per-payment allocation with cumulative and remaining-balance guards. | fixed and verified |
| JD-A-005 | judgment-day | `openspec/changes/unatienda-mvp/design.md:19-29` | CRITICAL | verified | Historical sale ownership and current-session return ownership are explicit without mutating closed-session rows. | fixed and verified |
| JD-A-006 | judgment-day | `openspec/changes/unatienda-mvp/design.md:14,69` | WARNING | info | SQLite `foreign_keys` and `busy_timeout` initialization is not explicitly stated per pooled connection. | real |
| JD-A-007 | judgment-day | `openspec/changes/unatienda-mvp/design.md:12,23` | WARNING | info | Aggregate `i64`/SQLite sums have no explicit checked-overflow policy. | theoretical |
| JD-B-001 | judgment-day | `openspec/changes/unatienda-mvp/design.md:71` | WARNING | info | Startup/resume-only scheduling can miss the required daily backup while the app stays continuously open. | real |

## Convergence

- Confirmed BLOCKER/CRITICAL: 0
- Open BLOCKER/CRITICAL: 0
- Verified BLOCKER/CRITICAL: 4
- Fixed pending re-review BLOCKER/CRITICAL: 0
- Contradictions: 0
- Informational warnings: 3
- Fix rounds used: 2/2

No further fix rounds remain, and the terminal state stays `JUDGMENT: ESCALATED`. Manual resolution selected the atomic idempotency model for JD-A-001; task planning may use that model without further Judgment Day review.

---

## Judgment Day: Foundation Slice 1 — Round 2

**State:** JUDGMENT: ESCALATED — fix budget exhausted; focused triage confirmed four remaining defects

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| JD-A-001 | judgment-day | `src-tauri/migrations/001_initial.sql:92-107` | BLOCKER | verified | All `payment_reversals` columns (incl. `created_at` with DEFAULT) precede the table-level `FOREIGN KEY` and `CHECK` constraints, matching SQLite's column-then-table-constraint grammar. The XOR `CHECK` (`(return_id IS NOT NULL AND annulment_sale_id IS NULL) OR (return_id IS NULL AND annulment_sale_id IS NOT NULL)`) is intact. `migration_creates_schema_tables` (asserts the table loads) and `payment_reversals_xor_source_constraint` (asserts valid return-only insert succeeds; both-sources and neither-source inserts fail) both pass. cargo test 23/23. | verified in final scoped re-review |
| JD-A-002 | judgment-day | `src-tauri/migrations/001_initial.sql:146-234` | BLOCKER | verified | Each invalid compound `BEFORE UPDATE OR DELETE` is split into separate `BEFORE UPDATE ON <t>` and `BEFORE DELETE ON <t>` triggers for payments (146-168), movements (170-188), returns (190-208), and payment_reversals (210-234). Every trigger keeps the `WHEN EXISTS (... status='closed' ...)` guard and `RAISE(ABORT, 'closed session records are immutable')`. `closed_session_immutability_triggers` asserts UPDATE and DELETE are blocked on payments, movements, returns, and payment_reversals. cargo test 23/23. | verified in final scoped re-review |
| JD-A-003 / JD-B-002 | judgment-day | `src-tauri/src/persistence/idempotency.rs:20-35,78-114` | BLOCKER | verified | Mapped `PoolError`/`InteractError` to `AppError::Database` explicitly; added `Send + 'static` bounds to `R` and `F` so `deadpool_sync::interact` accepts the closure. `cargo build` and `cargo fmt --check` pass. | confirmed and verified by both judges |
| JD-A-004 / JD-B-004 | judgment-day | `src-tauri/src/persistence/connection.rs:53-60` | BLOCKER | verified | Added `CreatePoolError` variant to `DbError` and switched `open_pool` to `Config::builder(...).post_create(...).build()` so pool construction errors are converted. `cargo build` and `cargo fmt --check` pass. | confirmed and verified by both judges |
| JD-A-005 / JD-B-001 | judgment-day | `src-tauri/Cargo.toml:23`; `src-tauri/src/persistence/connection.rs:67-79` | BLOCKER | verified | Enabled `backup` feature on `rusqlite`; replaced hand-rolled `backup.step(-1)` with `Connection::backup(DatabaseName::Main, &snapshot_path, None)` and removed the wrong `Connection::open`/`.to_string_lossy()` path handling. `cargo build` and `npx tauri build --debug --bundles app` pass. | confirmed and verified by both judges |
| JD-A-006 | judgment-day | `src-tauri/src/lib.rs:43-50` | BLOCKER | open | Synchronous setup calls `Handle::current()` and `block_on`; focused reliability triage confirmed this can panic without an entered runtime or when blocking inside one. | confirmed by focused triage |
| JD-A-007 / JD-B-007 | judgment-day | `src-tauri/tauri.conf.json:27-36` | BLOCKER | verified | Generated valid RGBA PNG/ICO/ICNS assets under `src-tauri/icons/` matching the configured paths. `npx tauri build --debug --bundles app` produced `UnaTienda.app` with `icon.icns` in Resources. | confirmed and verified by both judges |
| JD-A-008 | judgment-day | `src-tauri/src/domain/money.rs:12-14` | CRITICAL | open | `#[serde(transparent)]` over `i64` serializes `Money::from_cents(12345)` as `12345`, not required IPC string `"123.45"`; no Serde contract test exists. | confirmed by focused triage |
| JD-A-009 / JD-B-005 | judgment-day | `src-tauri/src/persistence/connection.rs:53-60` | CRITICAL | verified | Replaced first-connection-only initialization with a `post_create` hook on the pool builder; the hook synchronously locks the `Object` and runs `initialize_connection` so every newly created connection gets the required pragmas. `pool_initializes_pragmas` test passes. | confirmed and verified by both judges |
| JD-A-010 | judgment-day | `src-tauri/migrations/001_initial.sql:9-19,136-144` | CRITICAL | open | Executable SQLite triage changed a closed session amount, updated a closed-sale annulment reason, and deleted the sale; required finality is not enforced. | confirmed by focused triage |
| JD-B-003 | judgment-day | `src-tauri/src/persistence/idempotency.rs:63-74` | BLOCKER | refuted | Current bounds explicitly require `R` and `F` to be `Send + 'static`, satisfying deadpool interact requirements. | refuted by focused triage |
| JD-A-011 | judgment-day | `src-tauri/src/persistence/connection.rs:64-100` | WARNING | info | Repeated pre-migration snapshots may grow without cleanup. | real |
| JD-B-006 | judgment-day | `src-tauri/src/persistence/connection.rs:67-105` | CRITICAL | open | Every populated startup creates a timestamped snapshot; no migration-version gate, retention limit, or cleanup path exists, causing unbounded disk growth. | confirmed by focused triage |
| JD-B-008 | judgment-day | `src-tauri/src/persistence/connection.rs:45` | WARNING | info | `strict_tables` is not a stock SQLite pragma and does not make tables STRICT. | real |

### Foundation convergence

- Confirmed defect groups eligible for fixing: 5
- Verified converged groups: 5
- Verified Round 2 migration fixes: 2 (JD-A-001, JD-A-002)
- Open BLOCKER/CRITICAL after focused triage: 4
- Refuted findings: 1
- Informational warnings: 2 (preserved unchanged)
- Fix rounds used: 2/2 (budget exhausted)

Both judges verified the five converged Round 1 groups and the two Round 2 migration fixes. A later read-only focused reliability triage confirmed JD-A-006, JD-A-008, JD-A-010, and JD-B-006, and refuted JD-B-003. Because the Judgment Day fix budget is exhausted, the four confirmed defects remain open and the Foundation slice cannot be approved within this review loop.

---

## Judgment Day: Foundation Remediation Work Unit — Round 1

**State:** JUDGMENT: APPROVED — REM-A-001 verified by both judges; REM-A-002 remains informational

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| REM-A-001 | judgment-day | `src-tauri/migrations/002_upgrade_closed_session_immutability.sql:1-173`; `src-tauri/src/persistence/connection.rs:174-223,621-742` | CRITICAL | verified | Migration v2 replaces legacy triggers; the ordered runner records pending migrations atomically. Both judges verified the v1 upgrade test, fresh installs, snapshot gating, and 32/32 passing tests. | fixed and verified |
| REM-A-002 | judgment-day | `src-tauri/src/lib.rs:72-81` | SUGGESTION | info | The test exercises `initialize_app` directly rather than the complete Tauri setup closure. | theoretical |

### Remediation convergence

- Confirmed BLOCKER/CRITICAL: 0
- Confirmed CRITICAL: 0
- Verified CRITICAL: 1
- Fixed pending scoped re-review CRITICAL: 0
- Informational suggestions: 1
- Fix rounds used: 1/2

The legacy version-1 upgrade path is fixed and verified. Migration v2 safely replaces legacy triggers, all 32 Rust tests pass, and the remediation review is terminally approved.

---

## Foundation Remediation Work Unit (separate from Judgment Day fix rounds)

**State:** REMEDIATION VERIFIED — four defects fixed; fresh review approved after migration-v2 follow-up

This remediation is a newly user-authorized implementation unit. It is explicitly **not** Judgment Day fix round 3; the prior loop is terminally escalated and preserved above. The four defects below were fixed with behavior-first focused tests and full build verification.

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| JD-A-006 | remediation | `src-tauri/src/lib.rs:39-57` | BLOCKER | verified | Replaced `tokio::runtime::Handle::current().block_on` in Tauri setup with `tauri::async_runtime::block_on(initialize_app(&db_path))`. Extracted `initialize_app` as a plain async function that can be driven by any compatible executor. `tests::initialize_app_does_not_depend_on_tokio_handle_block_on` passes. | fixed and verified by fresh review |
| JD-A-008 | remediation | `src-tauri/src/domain/money.rs:66-95` | CRITICAL | verified | Removed `#[serde(transparent)]` and implemented custom `Serialize`/`Deserialize` using the existing `Display`/`FromStr` decimal-string contract. `Money::from_cents(12345)` now serializes as `"123.45"`; numeric cents and excess precision are rejected on deserialization. Four focused Serde contract tests pass. | fixed and verified by fresh review |
| JD-A-010 | remediation | `src-tauri/migrations/001_initial.sql:132-278` | CRITICAL | fixed | Enforced complete closed-session immutability: added `UPDATE`/`DELETE` guards on `cash_sessions` when `status='closed`; removed the column list on the `sales` update trigger so all columns (including `annulment_reason`) are protected; added `DELETE` guard on `sales`; added `UPDATE`/`DELETE` guards on `drafts`. Focused SQLite trigger test covers all tables and omitted fields. | fixed pending fresh review |
| JD-B-006 | remediation | `src-tauri/src/persistence/connection.rs:76-181` | CRITICAL | fixed | Pre-migration snapshots are now created only when migration version 1 is pending and the database already contains tables. Added `SNAPSHOT_RETENTION_LIMIT = 5` and `enforce_snapshot_retention` to keep only the newest five snapshots by filename timestamp. Three focused deterministic tests cover fresh-DB, pending-only, and retention behavior. | fixed pending fresh review |

### Remediation convergence

- Verified in this unit: 4 (JD-A-006, JD-A-008, JD-A-010, JD-B-006)
- Remaining open BLOCKER/CRITICAL from prior triage: 0
- Informational warnings preserved unchanged: JD-A-011, JD-B-008
- Prior terminal `JUDGMENT: ESCALATED` state preserved for history.

---

## Pre-commit Review: Foundation Work Unit — Round 1

**State:** ESCALATED — R3-002 verified; R3-001 remains open after the final scoped re-review

Standard pre-commit review of the staged Rust/Tauri Foundation work unit identified two focused defects. Both were fixed with behavior-first tests and full build verification. The source fixes remain staged with the existing Rust work unit; this ledger entry and `apply-progress.md` are unstaged pending the documentation commit.

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| R3-001 | pre-commit | `src-tauri/migrations/001_initial.sql:284-351`; `src-tauri/migrations/003_upgrade_closed_session_insert_guards.sql:1-84`; `src-tauri/src/persistence/connection.rs:1227-1503` | CRITICAL | verified | Added focused fresh-schema and genuine pre-v3 upgrade-path tests where the payment belongs to sale A/session A and `annulment_sale_id` points to a distinct sale B/session B. Both cases deliberately invert session A's state; the assertions prove annulment-based reversals follow sale B's session, not the payment's sale A. The tests pass under the current annulment-sale predicate and fail under the former payment-sale ownership predicate (verified by temporary SQL swap). Return-source and XOR coverage remain intact. cargo fmt --check, cargo test (41/41), and cargo build pass. | fixed and verified by test-only remediation |
| R3-002 | pre-commit | `src-tauri/src/domain/money.rs:65-75,101-159,275-318` | CRITICAL | verified | `Display` uses i128 magnitude; FromStr handles the exact i64 minimum and rejects out-of-domain values; Serde round-trips the full i64 domain. | fixed and verified |

### Pre-commit convergence

- Verified BLOCKER/CRITICAL: 2 (R3-001, R3-002)
- Open BLOCKER/CRITICAL: 0
- Pre-existing informational warnings preserved unchanged: JD-A-011, JD-B-008
- Fix rounds used: 2/2
- Test-only remediation: R3-001 annulment-sale ownership discrimination verified outside the exhausted fix rounds.

---

## Judgment Day: Phase 2 Cash Session Domain — Round 1

**State:** ESCALATED — blind judges did not converge on any BLOCKER/CRITICAL finding

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| JD2-A-001 | judgment-day | `src-tauri/src/persistence/cash_session_repo.rs:149-155`; `src-tauri/src/application/cash_session_use_case.rs:117-122` | CRITICAL | open | Judge A found return reversals attributed to the original sale session. Judge B found the same defect but classified it as a real WARNING deferred until Phase 5. | suspect; severity contradiction |
| JD2-A-002 | judgment-day | `src-tauri/src/application/cash_session_use_case.rs:107-150` | CRITICAL | open | Aggregate and draft-count query errors are swallowed with `unwrap_or`, allowing incorrect expected cash or bypassing the draft closure guard. | suspect; Judge A only |
| JD2-A-003 | judgment-day | `src-tauri/src/domain/cash_session.rs:85-90`; `src-tauri/src/commands/cash_session_commands.rs:35-44` | CRITICAL | open | Full-range Money input can overflow unchecked difference subtraction. | suspect; Judge A only |
| JD2-B-001 | judgment-day | `src/features/sessions/SessionView.tsx:43-57` | CRITICAL | open | A retry after a lost close response generates a new idempotency key and leaves stale UI state. Judge A found the same behavior but classified it as WARNING. | suspect; severity contradiction |
| JD2-I-001 | judgment-day | `src-tauri/src/application/cash_session_use_case.rs:188-200` | WARNING | info | Reconciliation reads the session and expected cash separately, allowing a close race to produce a stale zero view. | theoretical |
| JD2-I-002 | judgment-day | `src-tauri/src/commands/cash_session_commands.rs:18-20,40-42` | WARNING | info | Money parse failures return plain text instead of the documented structured error contract. | Judge A only |
| JD2-I-003 | judgment-day | `src/features/sessions/SessionView.tsx:93-95` | WARNING | info | `parseFloat` loses cent precision for large valid monetary values in the UI preview. | Judge A only |

### Round 1 convergence

- Confirmed BLOCKER/CRITICAL by both judges at the same severity: 0
- Suspect or severity-contradicted BLOCKER/CRITICAL candidates: 4
- Informational findings: 3
- Fixes applied: 0
- Terminal result: `JUDGMENT: ESCALATED`

---

## Phase 2 Cash Session Domain — Fix Round 1

**State:** FIXES APPLIED — four suspect CRITICAL candidates fixed with focused regression tests

User-directed "Investigar y corregir" fix round. Each candidate was reproduced, confirmed, fixed, and regression-tested.

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| JD2-A-001 | judgment-day | `src-tauri/src/persistence/cash_session_repo.rs:149-178`; `src-tauri/src/application/cash_session_use_case.rs:107-140` | CRITICAL | fixed | Reversal attribution SQL now joins through `returns.session_id` (for return-based) or `sales.session_id` of `annulment_sale_id` (for annulment-based), instead of the original payment's sale session. Two focused integration tests prove return-based and annulment-based reversals reduce the correct session's expected cash. `cargo test` 70/70. | fixed and tested |
| JD2-A-002 | judgment-day | `src-tauri/src/application/cash_session_use_case.rs:107-152` | CRITICAL | fixed | Replaced `.unwrap_or(opening_amount)` with `?` on expected_cash query and `.unwrap_or(0)` with `?` on draft_count query. Database errors now propagate as `AppError::Database` instead of being silently swallowed. Verified by existing `close_session_blocks_when_drafts_remain` test and the new reversal tests exercising the same code path. | fixed and tested |
| JD2-A-003 | judgment-day | `src-tauri/src/domain/cash_session.rs:85` | CRITICAL | fixed | Replaced `counted_cash - expected_cash` with `counted_cash.checked_sub(expected_cash).ok_or(AppError::Validation{..})`. Two domain tests (`close_session_rejects_overflowing_difference`, `close_session_accepts_valid_extreme_difference`) and one integration test (`close_session_rejects_overflowing_difference` in use_case) prove overflow is rejected and valid extremes succeed. | fixed and tested |
| JD2-B-001 | judgment-day | `src/features/sessions/SessionView.tsx:43-57` | CRITICAL | fixed | Added `useRef<string|null>(null)` to persist the idempotency key across retries. Key is generated once on first close attempt and reused on retry; cleared on success. Backend replay logic (same key + same request hash → replay stored response) now correctly applies to lost-response retries. | fixed (frontend logic; no automated test infrastructure for React components) |
| JD2-I-001 | judgment-day | `src-tauri/src/application/cash_session_use_case.rs:188-200` | WARNING | info | Unchanged. Informational; not addressed in this fix round. | theoretical |
| JD2-I-002 | judgment-day | `src-tauri/src/commands/cash_session_commands.rs:18-20,40-42` | WARNING | info | Unchanged. Informational; not addressed in this fix round. | theoretical |
| JD2-I-003 | judgment-day | `src/features/sessions/SessionView.tsx:93-95` | WARNING | info | Unchanged. Informational; not addressed in this fix round. | theoretical |

### Fix Round 1 convergence

- Fixed CRITICAL candidates: 4 (JD2-A-001, JD2-A-002, JD2-A-003, JD2-B-001)
- Informational warnings preserved unchanged: 3 (JD2-I-001..003)
- New regression tests: 5
- Fix rounds used: 1/2
- Verification: `cargo test` 70/70, `cargo fmt --check` clean, `npm run build` clean

### Scoped re-judgment

Both blind judges independently verified JD2-A-001, JD2-A-002, JD2-A-003, and JD2-B-001 against only the persisted ledger and fix-touched lines. No fix-introduced BLOCKER/CRITICAL findings were reported. Informational warnings remain unchanged.

- Verified CRITICAL findings: 4
- Open BLOCKER/CRITICAL findings: 0
- Fix rounds used: 1/2
- Terminal result: `JUDGMENT: APPROVED` ✅

---

## Pre-commit Review: Phase 2 Cash Session Domain

**State:** APPROVED — no surviving BLOCKER/CRITICAL findings

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| R3-P2-001 | reliability | `src/features/sessions/SessionView.test.tsx:199-227` | BLOCKER | refuted | General refuter confirmed that missing frontend retry coverage is not an implementation defect: `closeKeyRef` is retained on failure and cleared only on success, while backend tests prove same-key replay. | test coverage improvement only |
| R3-P2-002 | reliability | `src/features/sessions/SessionView.tsx:25-72` | WARNING | info | In-flight reconciliation requests can resolve out of order and display a stale difference. | real informational warning |
| R3-P2-003 | reliability | `src-tauri/src/commands/cash_session_commands.rs:18-20,40-42` | WARNING | info | Money parse errors use a different external error shape from downstream structured failures. | real informational warning |

### Pre-commit convergence

- Surviving BLOCKER/CRITICAL findings: 0
- Refuted BLOCKER/CRITICAL findings: 1
- Informational warnings: 2
- Verification: Rust 78/78; frontend 9/9; builds and lint pass

---

## Pre-push Review: Phase 2 Cash Session Domain

**State:** APPROVED — no surviving BLOCKER/CRITICAL findings

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| R3-PUSH-P2-001 | reliability | `src/features/sessions/SessionView.tsx:13`; `src/app/App.tsx:1-8` | BLOCKER | refuted | SessionView mounting is explicitly deferred to task 7.4. The formal Phase 2 report accepted task 2.5 and retained shell mounting as an informational warning. | deferred integration, not a Phase 2 defect |

### Pre-push convergence

- Surviving BLOCKER/CRITICAL findings: 0
- Refuted BLOCKER/CRITICAL findings: 1
- Verification: Rust 78/78; frontend 9/9; builds pass

### Initial remote Foundation pre-push check

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| R3-PUSH-FND-001 | reliability | `vitest.config.ts:5-10` | BLOCKER | refuted | No `.only` tests exist and all current tests pass. Missing `forbidOnly` is preventive tooling hardening, not a current implementation defect. | future hardening only |

---

## Judgment Day: V2-VERIFY-001 Remediation

**State:** APPROVED — both blind judges found no BLOCKER/CRITICAL defects

| id | lens | location | severity | status | evidence | assessment |
|---|---|---|---|---|---|---|
| V2-JD-A-001 | judgment-day | `src/features/sessions/SessionView.tsx:25-40,49-72,153-155` | WARNING | info | In-flight reconciliation responses can resolve out of order and stale differences may remain after an invalid input response. This does not invalidate the exact-money remediation. | real informational warning |

### Remediation convergence

- Open BLOCKER/CRITICAL findings: 0
- Informational warnings: 1
- Rust verification: 78/78 tests pass
- Frontend verification: 9/9 tests pass; build passes
- Terminal result: `JUDGMENT: APPROVED` ✅
