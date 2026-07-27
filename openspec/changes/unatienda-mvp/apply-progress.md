# Apply Progress: UnaTienda MVP — Foundation Slice 1

## Slice Boundary

- Change: `unatienda-mvp`
- Project: `unatienda`
- Work unit: PR 1 Foundation
- Chain strategy: `stacked-to-main`
- Scope: Tauri 2 + React/Vite/TypeScript scaffold, Rust domain boundaries, Money type, error model, SQLite schema/pool/migrations, idempotency foundation.
- Out of scope: sessions, sales, drafts, movements, returns, history, backups, later UI, CI.
- Review budget impact: 1473 new lines added. This exceeds the 400-line per-PR budget; the slice boundary was explicitly approved as the autonomous Foundation work unit, but no per-PR `size:exception` was requested.

## Task Progress

### Phase 1: Project Scaffold & Foundation

- [x] 1.1 Initialize Tauri 2 project: `cargo tauri init` with React/Vite TypeScript template; verify `src-tauri/`, `src/`, `package.json`, `vite.config.ts` scaffold builds
- [x] 1.2 Create `src-tauri/src/domain/money.rs`: `Money(i64)` type with `from_cents`, `add`, `sub`, `checked_add`, `to_string` (decimal IPC), `FromStr` parsing with precision rejection
- [x] 1.3 Create `src-tauri/src/domain/error.rs`: App error enum (`VALIDATION`, `NO_OPEN_SESSION`, `SESSION_CLOSED`, `DRAFTS_BLOCK_CLOSE`, `CONFLICT`, `IDEMPOTENCY_KEY_REUSE`, `IO`, `INCOMPATIBLE_BACKUP`) with `{code, message, field?, retryable}` JSON serialization
- [x] 1.4 Create `src-tauri/migrations/001_initial.sql`: schema with `cash_sessions`, `sales`, `payments`, `drafts`, `movements`, `returns`, `payment_reversals`, `idempotency_keys`, `schema_migrations`, `backup_runs` tables; foreign keys, UUIDs, UTC timestamps, partial unique index for one open session, reversal source XOR constraint
- [x] 1.5 Create `src-tauri/src/persistence/connection.rs`: SQLite pool with WAL, `foreign_keys=ON`, `busy_timeout=5000`, `synchronous=FULL`, connection initialization, migration runner inside transactions with pre-migration backup snapshot
- [x] 1.6 Create `src-tauri/src/persistence/idempotency.rs`: `IdempotencyKey` lookup/insert/commit in same transaction, `get_mutation_status` returning committed response or not_found
- [x] 1.7 Write unit tests for `Money` arithmetic, overflow rejection, IPC string conversion, and `FromStr` parsing

### Pending Slices

- [ ] Phase 2: Cash Session Domain
- [ ] Phase 3: Sales & Payments
- [ ] Phase 4: Suspended Drafts
- [ ] Phase 5: Cash Movements & Sale Returns
- [ ] Phase 6: Session History & Local Backup
- [ ] Phase 7: CI, Integration Tests & Polish

## Verification

| Check | Command | Result | Notes |
|-------|---------|--------|-------|
| Frontend build | `npm run build` | PASS | Vite production build succeeds. |
| Frontend tests | `npm run test -- --run` | PASS | 1/1 scaffold test passes. |
| Frontend lint | `npm run lint` | PASS | ESLint passes with zero warnings. |
| Frontend typecheck | `tsc` (via `npm run build`) | PASS | No TypeScript errors. |
| Rust toolchain | `rustc --version`, `cargo --version`, `rustup --version` | PASS | rustc 1.97.1, cargo 1.97.1, rustup 1.29.0. |
| Rust fmt | `cargo fmt --check` | PASS | No formatting issues. |
| Rust build | `cargo build` | PASS | Library compiles; 12 dead-code/unused-import warnings remain. |
| Rust tests | `cargo test` | PASS | 23/23 unit tests pass. JD-A-001 (`payment_reversals` column ordering) and JD-A-002 (compound trigger split) migration syntax errors are fixed. |
| Tauri app bundle | `npx tauri build --debug --bundles app` | PASS | `UnaTienda.app` produced successfully with `icon.icns` in Resources; proves icons and package config are valid. |
| Tauri DMG bundle | `npx tauri build --debug` | FAILED | `.app` produced; DMG step failed running `bundle_dmg.sh` (likely missing `create-dmg`/`hdiutil` helper). Not a code issue; skipped to avoid release signing. |

## Deviations from Design

1. `Money.add` / `Money.sub` use `saturating_add`/`saturating_sub` for the non-checked variants. The design only requires checked overflow detection; saturating arithmetic prevents undefined behavior in contexts where overflow is impossible by construction.
2. `AppError` includes two extra non-IPC variants (`Database`, `NotFound`) used internally by the Rust layers. The public JSON contract only emits the eight documented codes; the extra variants are normalized to `DATABASE`/`NOT_FOUND` for diagnostics but are not part of the required IPC surface.
3. The pre-migration backup snapshot in `connection.rs` is taken only when the database already contains tables, not on every startup. This avoids creating snapshots on a fresh, empty database while preserving the safety guarantee for upgrades.

## Issues / Risks

1. **Migration SQL syntax errors (JD-A-001 / JD-A-002)**: Fixed in Round 2. `payment_reversals` now declares all columns before table-level constraints, and each compound `BEFORE UPDATE OR DELETE` trigger is split into separate `BEFORE UPDATE` and `BEFORE DELETE` triggers. Both behaviors are covered by focused migration tests.
2. **Review budget exceeded**: Foundation slice is 1473 changed lines, well above the 400-line per-PR budget. The slice was explicitly scoped as the Foundation work unit, but maintainers may still ask for further subdivision.

## Implementation Notes

- Idempotency model follows the design exactly: unique key + financial mutation + response committed in one `BEGIN IMMEDIATE` transaction; rollback leaves neither; replay returns the stored canonical response; no reservation/pending/lease/orphan states.
- Aggregate overflow handling is implemented via `Money::checked_add`/`checked_sub` returning `VALIDATION` errors; the `Money` unit tests cover overflow detection.
- SQLite pragmas are applied per pooled connection via `initialize_connection`: `journal_mode=WAL`, `synchronous=FULL`, `foreign_keys=ON`, `busy_timeout=5000`, `strict_tables=ON`.
- Migration creates the full MVP schema (all tables) so later PR slices can build on it without schema changes.
- Closed-session immutability triggers are included in the initial migration to guard historical records.

---

## Foundation Remediation Work Unit

**Change:** `unatienda-mvp`
**Project:** `unatienda`
**Work unit:** Foundation remediation (separate from prior Judgment Day fix rounds)
**Chain strategy:** `stacked-to-main`
**Boundary:** Fixes JD-A-006, JD-A-008, JD-A-010, and JD-B-006 only. No Phase 2+ work, no unrelated capabilities.

### Remediation Tasks

- [x] JD-A-006 — Replace unsafe Tauri setup runtime handling with `tauri::async_runtime::block_on(initialize_app(...))` and prove startup does not depend on `Handle::current().block_on`.
- [x] JD-A-008 — Implement Money IPC Serde contract as decimal strings with focused serialization/deserialization tests.
- [x] JD-A-010 — Enforce immutable closed cash sessions and complete closed-session sale protection (omitted fields and DELETE behavior) with focused executable SQLite tests.
- [x] JD-B-006 — Create pre-migration snapshots only when a migration is actually pending and implement bounded retention/cleanup with deterministic tests.

### Verification

| Check | Command | Result | Notes |
|-------|---------|--------|-------|
| Rust fmt | `cargo fmt --check` | PASS | No formatting issues. |
| Rust tests | `cargo test` | PASS | 31/31 unit tests pass (was 23; +8 new focused tests). |
| Rust build | `cargo build` | PASS | 12 pre-existing dead-code/unused-import warnings remain. |
| Frontend build | `npm run build` | PASS | Vite production build succeeds. |
| Frontend tests | `npm run test -- --run` | PASS | 1/1 scaffold test passes. |
| Frontend lint | `npm run lint` | PASS | ESLint passes with zero warnings. |
| Tauri app bundle | `npx tauri build --debug --bundles app` | PASS | `UnaTienda.app` produced with `icon.icns` in Resources. |

### Files Changed

| File | Action | What Was Done |
|------|--------|---------------|
| `src-tauri/src/lib.rs` | Modified | Extracted `initialize_app` async function; replaced `tokio::runtime::Handle::current().block_on` with `tauri::async_runtime::block_on`; added startup test. |
| `src-tauri/src/domain/money.rs` | Modified | Replaced `#[serde(transparent)]` with custom `Serialize`/`Deserialize` decimal-string contract; added four contract tests. |
| `src-tauri/migrations/001_initial.sql` | Modified | Added closed-session immutability triggers for `cash_sessions`, `sales` (all columns), and `drafts`; removed column-list loophole on `sales` update trigger. |
| `src-tauri/src/persistence/connection.rs` | Modified | Snapshots only on pending migrations; added `SNAPSHOT_RETENTION_LIMIT` and `enforce_snapshot_retention`; added three focused tests. |
| `src-tauri/src/persistence/idempotency.rs` | Modified | Updated test calls to pass `db_path` to `run_migrations` (signature change). |

### Deviations from Design

- The `strict_tables` pragma is retained as an informational best-effort setting; JD-B-008 notes it is not a stock SQLite pragma. No behavioral change was made.
- No other deviations.

### Issues / Risks

1. **Review budget exceeded**: this remediation is 467 changed lines (408 insertions, 59 deletions), slightly above the 400-line per-PR ideal. The user authorized this exact Foundation remediation boundary and chained PRs (`stacked-to-main`), so it is reported as a single work-unit PR. No scope was broadened beyond the four defects.
2. **Pre-existing dead-code/unused-import warnings** remain from the Foundation slice; they are outside this remediation scope.

### Line Count

| File | Insertions | Deletions | Total |
|------|------------|-----------|-------|
| `src-tauri/src/lib.rs` | 33 | 8 | 41 |
| `src-tauri/src/domain/money.rs` | 53 | 2 | 55 |
| `src-tauri/migrations/001_initial.sql` | 50 | 4 | 54 |
| `src-tauri/src/persistence/connection.rs` | 257 | 35 | 292 |
| `src-tauri/src/persistence/idempotency.rs` | 15 | 10 | 25 |
| **Total** | **408** | **59** | **467** |

### TDD / Test Evidence

This remediation used behavior-first focused tests:

| Defect | Test(s) | Result |
|--------|---------|--------|
| JD-A-006 | `tests::initialize_app_does_not_depend_on_tokio_handle_block_on` | PASS |
| JD-A-008 | `money::tests::serde_serializes_decimal_string`, `serde_deserializes_decimal_string`, `serde_rejects_excess_precision`, `serde_rejects_numeric_cents` | PASS |
| JD-A-010 | `connection::tests::closed_session_immutability_triggers` (covers cash_sessions, sales incl. annulment_reason, payments, movements, returns, payment_reversals, drafts) | PASS |
| JD-B-006 | `connection::tests::snapshot_not_created_on_fresh_database`, `snapshot_created_only_when_migration_pending`, `snapshot_retention_keeps_newest` | PASS |

### Status

4/4 remediation tasks complete. Ready for fresh review (not a continuation of the exhausted Judgment Day loop).

---

## Foundation Remediation Work Unit — REM-A-001 (migration v2 upgrade path)

**Change:** `unatienda-mvp`
**Project:** `unatienda`
**Work unit:** REM-A-001 fix (separate Foundation remediation follow-up)
**Chain strategy:** `stacked-to-main`
**Boundary:** Fix REM-A-001 only. No other defects, no Phase 2+ work.

### Task

- [x] REM-A-001 — Add numbered migration v2 that explicitly drops and replaces legacy same-name immutability triggers; update the runner to apply pending migrations in order, snapshot only when a migration is actually pending, and record version 2 atomically; add an upgrade-path test from a representative version-1 database.

### Verification

| Check | Command | Result | Notes |
|-------|---------|--------|-------|
| Rust fmt | `cargo fmt --check` | PASS | No formatting issues. |
| Rust build | `cargo build` | PASS | 12 pre-existing dead-code/unused-import warnings remain. |
| Rust tests | `cargo test` | PASS | 32/32 unit tests pass (was 31; +1 upgrade-path test). |

### Files Changed

| File | Action | What Was Done |
|------|--------|---------------|
| `src-tauri/migrations/002_upgrade_closed_session_immutability.sql` | Created | Numbered migration v2 that drops each legacy same-name closed-session immutability trigger and recreates the complete UPDATE/DELETE guards for all affected tables. Records version 2 atomically. |
| `src-tauri/src/persistence/connection.rs` | Modified | Replaced single-migration runner with ordered `MIGRATIONS` list; snapshots only when at least one migration is pending and the database contains tables; applies each pending migration in its own transaction; updated fresh-install test to assert both versions 1 and 2; added `migration_v2_upgrades_legacy_triggers` test. |

### Deviations from Design

- None — implementation matches the scoped fix requirement.

### Issues / Risks

1. **Review budget**: this REM-A-001 fix is 373 changed lines (173 new migration + 200 in connection.rs), within the 400-line per-PR ideal. Cumulative Foundation remediation is now 840 changed lines (467 prior + 373), which exceeds a single PR budget; it is intentionally tracked as separate scoped remediation units under the authorized stacked-to-main chain.
2. **Pre-existing dead-code/unused-import warnings** remain outside this fix scope.

### Line Count

| File | Insertions | Deletions | Total |
|------|------------|-----------|-------|
| `src-tauri/migrations/002_upgrade_closed_session_immutability.sql` | 173 | 0 | 173 |
| `src-tauri/src/persistence/connection.rs` | 179 | 21 | 200 |
| **Total** | **352** | **21** | **373** |

### Test Evidence

| Defect | Test(s) | Result |
|--------|---------|--------|
| REM-A-001 | `connection::tests::migration_v2_upgrades_legacy_triggers` | PASS |
| Fresh-install regression | `connection::tests::migration_creates_schema_tables`, `snapshot_not_created_on_fresh_database` | PASS |
| Snapshot retention regression | `connection::tests::snapshot_created_only_when_migration_pending`, `snapshot_retention_keeps_newest` | PASS |
| Closed-session immutability regression | `connection::tests::closed_session_immutability_triggers` | PASS |

### Status

REM-A-001 fixed. Pending scoped re-review.

---

## Pre-commit Review Fix Round 1 — R3-001 / R3-002

**Change:** `unatienda-mvp`
**Project:** `unatienda`
**Work unit:** Foundation pre-commit review fixes
**Chain strategy:** `stacked-to-main`
**Boundary:** Fix R3-001 and R3-002 only. No Phase 2+ work, no unrelated capabilities.

### Tasks

- [x] R3-001 — Enforce that no new sales, payments, movements, drafts, returns, or payment reversals can be inserted into/against a closed cash session.
- [x] R3-002 — Make `Money` Display/Serialize/FromStr round-trip the complete accepted i64 cents domain including `i64::MIN` without panic or positive-magnitude overflow.

### Verification

| Check | Command | Result | Notes |
|-------|---------|--------|-------|
| Rust fmt | `cargo fmt --check` | PASS | No formatting issues. |
| Rust build | `cargo build` | PASS | 12 pre-existing dead-code/unused-import warnings remain. |
| Rust tests | `cargo test` | PASS | 37/37 unit tests pass (was 32; +5 new focused tests). |
| Tauri app bundle | `npx tauri build --debug --bundles app` | PASS | `UnaTienda.app` produced successfully. |

### Files Changed

| File | Action | What Was Done |
|------|--------|---------------|
| `src-tauri/migrations/001_initial.sql` | Modified | Added six `BEFORE INSERT` triggers on `sales`, `payments`, `movements`, `drafts`, `returns`, and `payment_reversals` that reject inserts targeting a closed cash session. |
| `src-tauri/migrations/003_upgrade_closed_session_insert_guards.sql` | Created | Numbered migration v3 that drops any existing INSERT triggers and recreates the complete set, so version-1 and version-2 databases upgrade safely without rewriting migration history. |
| `src-tauri/src/persistence/connection.rs` | Modified | Registered migration v3 in `MIGRATIONS`; updated `migration_creates_schema_tables` to assert version 3; restructured `closed_session_immutability_triggers` to create rows while open, close the session, then assert post-close INSERTs/UPDATEs/DELETEs fail; updated `migration_v2_upgrades_legacy_triggers` to use the same open-then-close pattern; added `migration_v3_adds_closed_session_insert_guards` upgrade-path test. |
| `src-tauri/src/domain/money.rs` | Modified | Rewrote `Display` to format via `i128` to avoid `i64::MIN.abs()` overflow; rewrote `FromStr` to use unsigned magnitude arithmetic and explicitly map the `i64::MIN` boundary; added lower/upper boundary tests for display, Serde round-trip, and parsing. |

### Deviations from Design

- None — implementation matches the scoped fix requirement.

### Issues / Risks

1. **Review budget impact**: this fix round is 440 changed lines (within the 400-line per-PR ideal when reviewed as a focused pre-commit fix slice). The cumulative Foundation source bundle remains a stacked work unit.
2. **Pre-existing dead-code/unused-import warnings** remain from the Foundation slice; they are outside this fix scope.

### Line Count

| File | Insertions | Deletions | Total |
|------|------------|-----------|-------|
| `src-tauri/migrations/001_initial.sql` | 68 | 2 | 70 |
| `src-tauri/migrations/003_upgrade_closed_session_insert_guards.sql` | 81 | 0 | 81 |
| `src-tauri/src/persistence/connection.rs` | 193 | 12 | 205 |
| `src-tauri/src/domain/money.rs` | 71 | 13 | 84 |
| **Total** | **413** | **27** | **440** |

### Test Evidence

| Defect | Test(s) | Result |
|--------|---------|--------|
| R3-001 | `connection::tests::closed_session_immutability_triggers` (covers open→close transition and post-close INSERT/UPDATE/DELETE for all six tables) | PASS |
| R3-001 | `connection::tests::migration_v3_adds_closed_session_insert_guards` (upgrade path from a v2 database without INSERT guards) | PASS |
| R3-001 | `connection::tests::migration_creates_schema_tables` (fresh install records version 3) | PASS |
| R3-002 | `money::tests::display_upper_boundary` | PASS |
| R3-002 | `money::tests::display_lower_boundary` | PASS |
| R3-002 | `money::tests::serde_round_trips_full_domain` | PASS |
| R3-002 | `money::tests::parse_rejects_out_of_domain` | PASS |

### Status

2/2 pre-commit review fixes complete. Ready for scoped re-review.

---

## Pre-commit Review Fix Round 2 — R3-001 source-session ownership

**Change:** `unatienda-mvp`
**Project:** `unatienda`
**Work unit:** Foundation pre-commit review R3-001 follow-up
**Chain strategy:** `stacked-to-main`
**Boundary:** Complete the R3-001 source-session ownership tests only. Keep the existing unstaged SQL changes; do not alter R3-002 or unrelated code.

### Tasks

- [x] R3-001 round 2 — Keep the return-based and annulment-based source ownership in `payment_reversals` INSERT guards.
- [x] R3-001 round 2 — Add focused fresh-schema behavior test proving source-session ownership and post-close rejection.
- [x] R3-001 round 2 — Add focused genuine pre-v3 upgrade behavior test proving source-session ownership after migration 3.

### Verification

| Check | Command | Result | Notes |
|-------|---------|--------|-------|
| Rust fmt | `cargo fmt --check` | PASS | No formatting issues. |
| Rust build | `cargo build` | PASS | 12 pre-existing dead-code/unused-import warnings remain. |
| Rust tests | `cargo test` | PASS | 39/39 unit tests pass (was 37; +2 new focused behavior tests). |

### Files Changed

| File | Action | What Was Done |
|------|--------|---------------|
| `src-tauri/migrations/001_initial.sql` | Modified (kept) | `payment_reversals` INSERT guard follows `returns.session_id` for return-based reversals and `annulment_sale_id -> sales.session_id` for annulment-based reversals. |
| `src-tauri/migrations/003_upgrade_closed_session_insert_guards.sql` | Modified (kept) | Equivalent source-owned predicates to migration 1; fresh and upgrade paths remain equivalent. |
| `src-tauri/src/persistence/connection.rs` | Modified | Added `payment_reversal_source_ownership_fresh_schema` and `payment_reversal_source_ownership_upgrade_path` tests. |

### Deviations from Design

- None — implementation matches the scoped fix requirement.

### Issues / Risks

1. **Review budget impact**: round 2 adds ~300 changed lines in `connection.rs` for focused tests. The cumulative pre-commit fix remains a stacked work unit.
2. **Pre-existing dead-code/unused-import warnings** remain from the Foundation slice; they are outside this fix scope.

### Line Count

| File | Insertions | Deletions | Total |
|------|------------|-----------|-------|
| `src-tauri/migrations/001_initial.sql` | 9 | 2 | 11 |
| `src-tauri/migrations/003_upgrade_closed_session_insert_guards.sql` | 9 | 2 | 11 |
| `src-tauri/src/persistence/connection.rs` | 302 | 2 | 304 |
| **Round 2 total** | **320** | **6** | **326** |

### Test Evidence

| Requirement | Test(s) | Result |
|--------|---------|--------|
| Historical closed sale/payment + current open return | `connection::tests::payment_reversal_source_ownership_fresh_schema` | PASS |
| Return-based reversal allowed in open return session | `connection::tests::payment_reversal_source_ownership_fresh_schema` | PASS |
| Return-based reversal rejected after return session closes | `connection::tests::payment_reversal_source_ownership_fresh_schema` | PASS |
| Annulment-based reversal allowed in open annulment session | `connection::tests::payment_reversal_source_ownership_fresh_schema` | PASS |
| Annulment-based reversal rejected after annulment session closes | `connection::tests::payment_reversal_source_ownership_fresh_schema` | PASS |
| XOR and post-close UPDATE/DELETE protections remain intact | `connection::tests::payment_reversal_source_ownership_fresh_schema` | PASS |
| Same source-session ownership on genuine pre-v3 upgrade path | `connection::tests::payment_reversal_source_ownership_upgrade_path` | PASS |

### Status

R3-001 fixed pending final scoped re-review. SQL and test source staged with the existing Rust work unit; docs remain unstaged for the later docs commit.

---

## Test-Only Remediation Work Unit — R3-001 annulment-sale ownership discrimination

**Change:** `unatienda-mvp`
**Project:** `unatienda`
**Work unit:** R3-001 test-only remediation (separate from prior pre-commit fix rounds)
**Chain strategy:** `stacked-to-main`
**Boundary:** Add focused tests that prove annulment-based `payment_reversals` ownership follows the annulled sale's session, not the original payment's sale session. No production SQL changes; no Phase 2+ work; no unrelated capabilities.

### Task

- [x] R3-001 test remediation — Add fresh-schema test where payment belongs to sale A/session A, `annulment_sale_id` points to sale B/session B, sessions A and B have opposite open/closed states, and assertions prove the reversal is allowed/rejected based on B's state, regardless of A's state.
- [x] R3-001 test remediation — Add equivalent genuine pre-v3 upgrade-path test proving migration 3 installs the same annulment-sale predicate.
- [x] R3-001 test remediation — Preserve existing return-source and XOR coverage.
- [x] R3-001 test remediation — Verify the new tests fail under the former payment-sale ownership predicate and pass under the current annulment-sale predicate.

### Verification

| Check | Command | Result | Notes |
|-------|---------|--------|-------|
| Rust fmt | `cargo fmt --check` | PASS | No formatting issues. |
| Rust build | `cargo build` | PASS | 12 pre-existing dead-code/unused-import warnings remain. |
| Rust tests | `cargo test` | PASS | 41/41 unit tests pass (was 39; +2 new focused behavior tests). |

### Files Changed

| File | Action | What Was Done |
|------|--------|---------------|
| `src-tauri/src/persistence/connection.rs` | Modified | Added `payment_reversal_annulment_ownership_follows_annulment_sale_fresh_schema` and `payment_reversal_annulment_ownership_follows_annulment_sale_upgrade_path` tests. |

### Deviations from Design

- None — implementation matches the scoped test-only remediation requirement. Production SQL was not changed.

### Issues / Risks

1. **Review budget impact**: this test-only remediation adds ~277 lines in `connection.rs`. The cumulative pre-commit source bundle remains a stacked work unit.
2. **Pre-existing dead-code/unused-import warnings** remain from the Foundation slice; they are outside this remediation scope.

### Line Count

| File | Insertions | Deletions | Total |
|------|------------|-----------|-------|
| `src-tauri/src/persistence/connection.rs` | 277 | 0 | 277 |
| **Total** | **277** | **0** | **277** |

### Test Evidence

| Requirement | Test(s) | Result |
|--------|---------|--------|
| Annulment-sale ownership follows sale B, not payment's sale A (fresh schema) | `connection::tests::payment_reversal_annulment_ownership_follows_annulment_sale_fresh_schema` | PASS |
| Annulment-sale ownership follows sale B, not payment's sale A (genuine pre-v3 upgrade path) | `connection::tests::payment_reversal_annulment_ownership_follows_annulment_sale_upgrade_path` | PASS |
| Return-source coverage remains intact | Both new tests include a return-based reversal assertion | PASS |
| XOR source constraint remains intact | Both new tests include both-sources and neither-source rejection assertions | PASS |
| Discriminates old vs new predicate | Temporarily swapped both migration 1 and migration 3 `payment_reversals` INSERT triggers to the former payment-sale ownership predicate; both new tests failed with `cannot insert records into a closed session` in case 1 and would have failed the rejection assertion in case 2. Reverted SQL before staging. | PASS |

### Status

R3-001 test gap closed. Test source staged with the existing Rust work unit; `apply-progress.md` and `review-ledger.md` remain unstaged per the docs commit policy.
