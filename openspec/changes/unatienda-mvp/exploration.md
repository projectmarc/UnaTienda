# Exploration: UnaTienda MVP — Greenfield POS Application

## Summary

UnaTienda is a greenfield desktop point-of-sale/cash-register application for a small shop, primarily used on a Windows counter PC. The stack is already decided: Tauri 2 + React/TypeScript/Vite + Rust use-case/persistence layer + SQLite. This exploration maps the MVP feature surface, analyzes architectural options for a Tauri monolith, and identifies UI/workflow considerations from the interface-design skill.

## Current State

**Project state:** Empty git repo — no source code, no commits, no dependencies.

**Decisions already captured (Engram):**
- Stack: Tauri 2, React + TypeScript + Vite, Rust, SQLite
- Architecture principle: React must NOT contain critical cash logic — Rust owns transactions, validations, and backups
- Avoid: Next.js, Supabase, remote backend in MVP
- CI: Windows installers built on Windows runners; unsigned installer may trigger SmartScreen warnings

**OpenSpec state:** Config exists but specs directory is empty. No prior changes archived.

## Product Domain

**Who is this human?** A small-shop owner or cashier at a physical counter. They work standing or seated at a fixed Windows PC. They may not be technically sophisticated. They need to open the register, make sales, record movements, and close out — fast, without errors.

**What must they accomplish?**
1. Open a cash session with an editable default float
2. Record sales — accounting total is the primary amount; product details are optional and may be incomplete
3. Record income, expense, and withdrawal movements (cash/manual)
4. Close the session with reconciliation: expected cash vs. counted cash, over/short difference

**What should it feel like?** Like a well-worn notebook — fast, forgiving, no friction. Not like enterprise software. The interface should be calm, tactile, and focused on the current session. Numbers should be large and scannable. The workflow should feel linear: open → work → close.

**Key domain concepts:**
- **Cash session** — a bounded period from opening to closing; the core organizational unit
- **Opening float** — editable default amount of cash in the drawer at session start
- **Sale** — a transaction with an accounting total (mandatory) and optional product details
- **Movement** — income, expense, or withdrawal; always tied to a session
- **Reconciliation** — comparing expected cash (opening + sales + income - expenses - withdrawals) against counted cash
- **Over/short** — the delta between expected and counted; the key output of closing

**Product catalog reality:** Many products lack SKU or digital price. The POS must work with incomplete catalog data — sale entry must not require product lookup.

## Architecture Analysis

### Tauri 2 Monolith Structure

Since this is a desktop app with no server dependency, the entire application is a Tauri monolith:

```
una-tienda/
├── src-tauri/           # Rust backend
│   ├── src/
│   │   ├── main.rs      # Tauri entry point
│   │   ├── commands/    # Tauri command handlers (IPC bridge)
│   │   ├── domain/      # Business logic (sessions, sales, movements, reconciliation)
│   │   ├── persistence/ # SQLite repository implementations
│   │   └── db/          # Schema, migrations
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                 # React frontend
│   ├── App.tsx
│   ├── pages/           # Session views (open, sell, movements, close)
│   ├── components/      # UI components
│   ├── hooks/           # Custom hooks for Tauri command invocation
│   └── types/           # TypeScript interfaces (mirrors Rust domain types)
├── package.json
└── vite.config.ts
```

### Key Architectural Decision: Where Does Logic Live?

The existing decision is clear: **Rust owns critical cash logic**. This means:
- All monetary calculations happen in Rust
- Session state transitions (open → active → closing → closed) are enforced in Rust
- Reconciliation logic (expected = opening + sales + income - expenses - withdrawals) is in Rust
- React is a thin presentation layer: display state, capture input, invoke commands

This is the right call. A POS system where the UI could bypass validation or miscalculate totals is a liability.

### IPC Boundary (Tauri Commands)

The Tauri command layer is the contract between frontend and backend. Each command maps to a use case:

| Command | Purpose |
|---------|---------|
| `open_session(opening_float)` | Start new cash session with editable float |
| `get_active_session()` | Retrieve current open session (if any) |
| `record_sale(amount, description?)` | Record a sale in the active session |
| `record_movement(type, amount, description?)` | Record income/expense/withdrawal |
| `close_session(counted_cash)` | Close session with reconciliation |
| `get_session_summary()` | Get current session totals for closing view |

This is a small, focused API surface — good for an MVP.

### SQLite Schema (Draft)

```sql
CREATE TABLE cash_sessions (
    id INTEGER PRIMARY KEY,
    opening_float REAL NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',  -- open | closing | closed
    opened_at TEXT NOT NULL,
    closed_at TEXT,
    counted_cash REAL,
    expected_cash REAL,
    over_short REAL
);

CREATE TABLE transactions (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES cash_sessions(id),
    type TEXT NOT NULL,  -- sale | income | expense | withdrawal
    amount REAL NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL
);
```

Simple, minimal, sufficient for MVP.

## UI / Workflow Analysis

### Workflow Map (Linear)

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  OPEN       │────▶│  SELL /      │────▶│  CLOSE      │────▶│  DONE       │
│  SESSION    │     │  MOVEMENTS   │     │  SESSION    │     │             │
└─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘
```

**Open:** One screen. Editable opening float (default from last session or configured default). Big "Open Session" button.

**Sell / Movements:** Main working screen. Two primary actions: "New Sale" and "New Movement". Session summary visible at all times (total sales, total movements, current expected cash). Sales and movements listed chronologically.

**Close:** One screen. Shows expected cash. Input field for counted cash. Over/short calculated live. Big "Close Session" button.

### Interface Design Considerations

From the interface-design skill's domain exploration:

**Color world of a small shop cash register:**
- The warm beige/cream of paper receipts
- The green of US dollar bills (or local currency)
- The dark gray/black of a POS terminal
- The amber/orange of a cash drawer indicator light
- The white of receipt paper

**Signature element:** The session timeline — a visual representation of the session's flow showing opening, sales, movements, and the current expected cash at a glance. This is the heartbeat of the session.

**Defaults to reject:**
1. Standard dashboard sidebar navigation → Replace with linear workflow (open → work → close)
2. Generic "Add" buttons → Replace with contextual primary actions (New Sale, New Movement)
3. Table-heavy data display → Replace with large, scannable number cards for session totals

**Typography intent:** Monospace for monetary amounts (tabular alignment, receipt feel). Sans-serif for labels and descriptions. Large numbers for session totals — the cashier should be able to read them from a standing position.

**Depth strategy:** Borders-only. Dense tool, technical feel. No shadows — this is a work tool, not a consumer app.

### Touch / Input Considerations

The primary device is a Windows counter PC. Likely uses mouse and keyboard, but touch is possible on some setups. Sale entry must be fast — large touch targets for primary actions, keyboard shortcuts for power users.

## Affected Areas

Since this is greenfield, everything is new. The exploration identifies these build areas:

| Area | Files | Notes |
|------|-------|-------|
| Tauri scaffold | `src-tauri/`, `tauri.conf.json` | Project initialization, window config, IPC setup |
| Rust domain | `src-tauri/src/domain/` | Session, sale, movement, reconciliation logic |
| Rust persistence | `src-tauri/src/persistence/` | SQLite repositories, migrations |
| Rust commands | `src-tauri/src/commands/` | Tauri IPC command handlers |
| React shell | `src/` | App shell, routing, Tauri command hooks |
| Open session | `src/pages/OpenSession.tsx` | Opening float input, session creation |
| Sell / movements | `src/pages/SessionWork.tsx` | Main working screen, sale/movement entry |
| Close session | `src/pages/CloseSession.tsx` | Reconciliation, over/short display |
| CI / CD | `.github/workflows/` | Windows build, installer generation |

## Approaches

### Approach A: Bottom-Up (Rust First)

Build the Rust domain and persistence layers first, test them in isolation, then wire up the React UI.

- **Pros:** Solid foundation, domain logic verified before UI exists, easier to test
- **Cons:** No visible progress until late, harder to validate UX decisions early
- **Effort:** Medium

### Approach B: Top-Down (UI First)

Build the React UI with mock data first, validate the workflow and layout, then implement Rust backend.

- **Pros:** Early visual feedback, validates workflow decisions, easier to iterate on UX
- **Cons:** Mock data may not match real behavior, risk of rework when wiring real backend
- **Effort:** Medium

### Approach C: Vertical Slices (Recommended)

Build one complete feature at a time, from UI to database. Start with "Open Session" (simplest), then "Record Sale", then "Close Session", then movements.

- **Pros:** Each slice is independently testable and demonstrable, natural progression, early feedback on real behavior
- **Cons:** Requires discipline to not over-build each slice, some rework as later slices reveal needs
- **Effort:** Medium

## Recommendation

**Approach C: Vertical Slices.**

Reasoning:
1. The MVP is small enough that vertical slices are natural — each slice is one Tauri command plus one screen.
2. Each slice produces a working, testable increment — critical for a solo/small-team greenfield project.
3. The linear workflow (open → sell → close) maps directly to slice ordering.
4. Rust logic can be unit-tested within each slice without waiting for the full UI.

**Suggested slice order:**
1. **Scaffold** — Tauri project, React shell, SQLite init, CI skeleton
2. **Open Session** — Rust: create session command; UI: opening float input
3. **Record Sale** — Rust: add sale command; UI: sale entry form, session totals
4. **Record Movement** — Rust: add movement command; UI: movement entry form
5. **Close Session** — Rust: close/reconcile command; UI: reconciliation screen
6. **Session History** — Rust: query past sessions; UI: simple session list (optional for MVP, but useful)

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Unsigned Windows installer** | SmartScreen warnings erode user trust | Accept for MVP; investigate code signing options before release |
| **SQLite data loss on crash** | Cash session data could be lost | Use WAL mode, periodic checkpoints, test crash recovery |
| **React bypasses Rust validation** | Monetary miscalculation | Enforce all calculations in Rust; React only displays and invokes commands |
| **Incomplete product catalog** | Sale entry must not require product lookup | Design sale entry for manual amount entry as primary mode; catalog search is optional enhancement |
| **Scope creep into inventory/reporting** | MVP delays | Strict scope: no inventory decrement, no operator PINs, no reporting website in v1 |
| **Tauri 2 ecosystem maturity** | Fewer examples/plugins than Electron | Tauri 2 is stable; SQLite plugin is well-supported; Windows-specific issues are known and documented |

## Ready for Proposal

**Yes.** The exploration has sufficient clarity to proceed to proposal. The orchestrator should tell the user:

1. The MVP scope is well-defined: 4 core workflows (open, sell, movements, close)
2. The architecture is clear: Tauri 2 monolith, Rust owns logic, React is presentation
3. The recommended approach is vertical slices, starting with the simplest (Open Session)
4. The main risks are manageable: unsigned installer, crash recovery, scope discipline
5. Interface design direction: linear workflow, large numbers, monospace amounts, borders-only depth

The next step is `sdd-propose` to formalize the change scope, approach, and rollback plan.
