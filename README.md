<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" alt="UnaTienda" width="96" height="96">
</p>

<h1 align="center">UnaTienda</h1>

<p align="center">
  <strong>A fast, forgiving, offline-first cash register for small shops.</strong><br>
  Open the till, sell, close the day. No server, no accounts, no internet required.
</p>

<p align="center">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2021-000000?logo=rust&logoColor=white">
  <img alt="React 18" src="https://img.shields.io/badge/React-18-61DAFB?logo=react&logoColor=black">
  <img alt="SQLite" src="https://img.shields.io/badge/SQLite-local-003B57?logo=sqlite&logoColor=white">
  <img alt="Status" src="https://img.shields.io/badge/status-MVP%20in%20progress-orange">
</p>

---

## Why

Most point-of-sale software is built for chains: logins, cloud sync, inventory, and a subscription. A small shop counter usually needs something simpler. It needs to know how much cash is in the drawer, what came in by transfer or card, and whether the numbers match at closing time.

UnaTienda does that and stays out of the way. Everything lives on the counter PC, in a local SQLite database you can back up and restore yourself.

## What it does

| | Feature | Details |
|---|---|---|
| 🔓 | **Open the till** | One open cash session per computer, starting from a counted opening amount. |
| 🧾 | **Record sales fast** | Only the total is required. Details and notes are optional. |
| 💳 | **Split payments** | Mix cash, transfer, debit, and credit in one sale. You can tag a transfer with the receiving account. |
| 💸 | **Cash out** | Record withdrawals from the drawer during the session. |
| ↩️ | **Annul sales** | Annul a sale while its session is open. Nothing is deleted: annulled sales stay in the record. |
| 📊 | **Reconcile and close** | Compare expected and counted cash, accept the difference, and close. A closed session is final and cannot be changed. |
| 🕓 | **History** | Browse past sessions with their sales, movements, and per-account transfer totals. |
| 💾 | **Backups** | Export a copy of the database and restore it safely when you need to. |

The interface is in Spanish (`Ventas`, `Caja actual`, `Historial`, `Movimientos`), because it was built for shops in Argentina.

### On the roadmap

- Suspended sale drafts (park a sale, serve the next customer, resume it)
- Full and partial returns linked to the original sale
- Income and expense movements
- Automatic daily backups
- Windows installers built in CI

## Design principles

- **Local first.** Day-to-day operation never depends on the network.
- **Forgiving.** Few required fields and no customer receipts or operator PINs. Closing differences are allowed and recorded instead of blocked.
- **Traceable.** Financial records are never silently erased. Annulments and closures leave a trail.
- **Money is Rust's job.** All amounts and session lifecycle rules live in the Rust core. The UI only displays them.

## Architecture

```
┌──────────────────────────────┐
│  React + TypeScript + Vite   │  src/            UI, Tailwind, Base UI
└──────────────┬───────────────┘
               │ Tauri IPC (amounts sent as strings)
┌──────────────▼───────────────┐
│  commands/                   │  src-tauri/src/  IPC boundary
│  application/                │                  use cases and idempotency
│  domain/                     │                  cash and sale rules
│  persistence/  backup/       │                  SQLite (WAL) and backups
└──────────────────────────────┘
```

SQLite schema changes are versioned in [`src-tauri/migrations/`](src-tauri/migrations). Database triggers make closed sessions immutable, on top of the checks in the application.

## Getting started

### Prerequisites

- [Node.js](https://nodejs.org/) 20+
- [Rust](https://www.rust-lang.org/tools/install), stable toolchain
- The [Tauri 2 system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS

### Run it

```bash
npm install
npm run tauri:dev
```

### Build an installer

```bash
npm run tauri:build
```

The output goes to `src-tauri/target/release/bundle/`.

## Development

| Command | What it does |
|---|---|
| `npm run tauri:dev` | Run the desktop app with hot reload |
| `npm run dev` | Run only the web UI in the browser (no Rust backend) |
| `npm test` | Frontend tests (Vitest + Testing Library) |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Rust domain, persistence, and integration tests |
| `npm run lint` | ESLint, with zero warnings allowed |
| `npm run format` | Prettier |

Specs, design notes, and the task plan for the MVP are in [`openspec/changes/unatienda-mvp/`](openspec/changes/unatienda-mvp). The visual design contract is in [`.interface-design/system.md`](.interface-design/system.md).

## Project layout

```
src/
  app/              application shell and navigation
  components/ui/    shared UI primitives
  features/
    sessions/       opening, sales, history, closing
    backups/        export and restore
src-tauri/
  src/              Rust core (domain, application, persistence, commands)
  migrations/       SQLite schema migrations
openspec/           specs and design documents
```

---

<p align="center">Made for the shop counter. ☕</p>
