# Proposal: UnaTienda MVP

## Intent

Deliver a fast, forgiving, local-first Windows POS that preserves traceable offline records for a small shop.

## Scope

### In Scope
- Enforce one open session per computer; accept closing differences; make closure immutable and final.
- Require only a sale total. Keep details/notes optional and support split cash, transfer, debit, and credit payments; transfer accounts remain optional.
- Suspend, resume, or discard multiple non-posting drafts. Drafts cannot cross sessions and block closing.
- Record income, expenses, and withdrawals. Retain annulled sales after confirmation without requiring a reason.
- Link total/partial returns to confirmed sales and reverse the correct payment methods.
- Browse sessions and sale/movement details; create internal records without customer receipts.
- Run automatic daily backups and provide manual export.

### Out of Scope
- Inventory decrement/control, operator users or PINs, an authenticated reporting website, cloud operational dependency, and OpenPencil.

## Capabilities

### New Capabilities
- `cash-session-management`: Open, reconcile, and close the single session.
- `sales-and-payments`: Confirm, annul, and split-tender sales.
- `suspended-sale-drafts`: Preserve multiple active-session drafts.
- `cash-movements`: Record income, expenses, and withdrawals.
- `sale-returns`: Trace returns and payment reversals.
- `session-history`: Browse sessions and transaction details.
- `local-backup`: Schedule and export backups.

### Modified Capabilities
None; no existing specifications exist.

## Approach

Use a Tauri 2 monolith: React/TypeScript/Vite presents tasks, Rust owns monetary and lifecycle rules, and SQLite provides local durability. This fits the offline Windows-counter goal without a server. Deliver vertical slices through open → work → close. Favor a session timeline, scannable amounts, and borders-only depth.

## Affected Areas

| Area | Impact | Description |
|---|---|---|
| `src-tauri/src/domain/` | New | Cash rules |
| `src-tauri/src/persistence/`, `src-tauri/src/db/` | New | Storage and backups |
| `src-tauri/src/commands/` | New | IPC boundary |
| `src/` | New | Desktop interface |
| `.github/workflows/` | New | Windows builds |

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Data loss | Medium | WAL, atomic writes, restore-tested backups |
| Financial defects | Medium | Rust invariants and boundary tests |
| Draft/return ambiguity | Medium | Specify lifecycle and reversal scenarios next |
| SmartScreen warnings | High | Document warnings; defer signing |

## Rollback Plan

Stop distribution, restore the prior build, and recover a verified backup. Back up before migrations; retain annulled financial records.

## Dependencies

- Tauri 2/Rust and Node toolchains, SQLite, and a Windows CI runner.

## Success Criteria

- [ ] In-scope workflows work offline and enforce Rust-owned rules.
- [ ] Sessions, annulments, returns, drafts, and history remain consistent and traceable.
- [ ] Daily and exported backups restore successfully.
