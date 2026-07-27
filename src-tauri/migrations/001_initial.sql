-- Initial schema for UnaTienda MVP.
-- Money is stored in INTEGER cents; UUIDs are TEXT; timestamps are UTC.

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS cash_sessions (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL CHECK (status IN ('open', 'closed')),
    opening_amount INTEGER NOT NULL,
    counted_cash INTEGER,
    difference INTEGER,
    opened_at TEXT NOT NULL,
    closed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Enforce at most one open session per computer.
CREATE UNIQUE INDEX IF NOT EXISTS idx_one_open_session
    ON cash_sessions(status) WHERE status = 'open';

CREATE TABLE IF NOT EXISTS sales (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    total_amount INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'confirmed' CHECK (status IN ('confirmed', 'annulled')),
    details TEXT,
    notes TEXT,
    annulment_reason TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY (session_id) REFERENCES cash_sessions(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_sales_session_id ON sales(session_id);

CREATE TABLE IF NOT EXISTS payments (
    id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL,
    method TEXT NOT NULL CHECK (method IN ('cash', 'transfer', 'debit', 'credit')),
    amount INTEGER NOT NULL,
    account TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY (sale_id) REFERENCES sales(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_payments_sale_id ON payments(sale_id);

CREATE TABLE IF NOT EXISTS drafts (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    total_amount INTEGER,
    details TEXT,
    notes TEXT,
    payments_json TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY (session_id) REFERENCES cash_sessions(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_drafts_session_id ON drafts(session_id);

CREATE TABLE IF NOT EXISTS movements (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('income', 'expense', 'withdrawal')),
    amount INTEGER NOT NULL,
    note TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY (session_id) REFERENCES cash_sessions(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_movements_session_id ON movements(session_id);

CREATE TABLE IF NOT EXISTS returns (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    sale_id TEXT NOT NULL,
    return_kind TEXT NOT NULL CHECK (return_kind IN ('total', 'partial')),
    amount INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY (session_id) REFERENCES cash_sessions(id) ON DELETE RESTRICT,
    FOREIGN KEY (sale_id) REFERENCES sales(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_returns_session_id ON returns(session_id);
CREATE INDEX IF NOT EXISTS idx_returns_sale_id ON returns(sale_id);

CREATE TABLE IF NOT EXISTS payment_reversals (
    id TEXT PRIMARY KEY,
    payment_id TEXT NOT NULL,
    amount INTEGER NOT NULL,
    -- A reversal is caused by exactly one of: an annulment of its sale, or a return.
    return_id TEXT,
    annulment_sale_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY (payment_id) REFERENCES payments(id) ON DELETE RESTRICT,
    FOREIGN KEY (return_id) REFERENCES returns(id) ON DELETE RESTRICT,
    FOREIGN KEY (annulment_sale_id) REFERENCES sales(id) ON DELETE RESTRICT,
    CHECK (
        (return_id IS NOT NULL AND annulment_sale_id IS NULL)
        OR (return_id IS NULL AND annulment_sale_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_payment_reversals_payment_id ON payment_reversals(payment_id);
CREATE INDEX IF NOT EXISTS idx_payment_reversals_return_id ON payment_reversals(return_id);
CREATE INDEX IF NOT EXISTS idx_payment_reversals_annulment_sale_id ON payment_reversals(annulment_sale_id);

CREATE TABLE IF NOT EXISTS idempotency_keys (
    idempotency_key TEXT PRIMARY KEY,
    command TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    response_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS backup_runs (
    id TEXT PRIMARY KEY,
    run_at TEXT NOT NULL,
    path TEXT NOT NULL,
    success INTEGER NOT NULL CHECK (success IN (0, 1)),
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_backup_runs_run_at ON backup_runs(run_at);

-- Guard closed-session immutability for rows owned by a closed session.
-- Updates/deletes on cash_sessions, sales, payments, movements, drafts, returns,
-- and payment_reversals are rejected when their parent session is closed.
-- Inserts into sales, payments, movements, drafts, returns, and payment_reversals
-- are also rejected when they target a closed session.

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_cash_sessions_update
BEFORE UPDATE ON cash_sessions
FOR EACH ROW
WHEN OLD.status = 'closed'
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_cash_sessions_delete
BEFORE DELETE ON cash_sessions
FOR EACH ROW
WHEN OLD.status = 'closed'
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_sales_update
BEFORE UPDATE ON sales
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_sales_delete
BEFORE DELETE ON sales
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_payments_update
BEFORE UPDATE ON payments
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions cs
    JOIN sales s ON s.session_id = cs.id
    WHERE s.id = OLD.sale_id AND cs.status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_payments_delete
BEFORE DELETE ON payments
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions cs
    JOIN sales s ON s.session_id = cs.id
    WHERE s.id = OLD.sale_id AND cs.status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_movements_update
BEFORE UPDATE ON movements
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_movements_delete
BEFORE DELETE ON movements
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_drafts_update
BEFORE UPDATE ON drafts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_drafts_delete
BEFORE DELETE ON drafts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_returns_update
BEFORE UPDATE ON returns
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_returns_delete
BEFORE DELETE ON returns
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_payment_reversals_update
BEFORE UPDATE ON payment_reversals
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions cs
    JOIN payments p ON p.id = OLD.payment_id
    JOIN sales s ON s.id = p.sale_id
    WHERE cs.id = s.session_id AND cs.status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_payment_reversals_delete
BEFORE DELETE ON payment_reversals
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions cs
    JOIN payments p ON p.id = OLD.payment_id
    JOIN sales s ON s.id = p.sale_id
    WHERE cs.id = s.session_id AND cs.status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

-- Guard inserts against closed sessions.
CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_sales_insert
BEFORE INSERT ON sales
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot insert records into a closed session');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_payments_insert
BEFORE INSERT ON payments
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions cs
    JOIN sales s ON s.session_id = cs.id
    WHERE s.id = NEW.sale_id AND cs.status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot insert records into a closed session');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_movements_insert
BEFORE INSERT ON movements
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot insert records into a closed session');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_drafts_insert
BEFORE INSERT ON drafts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot insert records into a closed session');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_returns_insert
BEFORE INSERT ON returns
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot insert records into a closed session');
END;

CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_payment_reversals_insert
BEFORE INSERT ON payment_reversals
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM returns r
    JOIN cash_sessions cs ON cs.id = r.session_id
    WHERE r.id = NEW.return_id AND cs.status = 'closed'
) OR EXISTS (
    SELECT 1 FROM sales s
    JOIN cash_sessions cs ON cs.id = s.session_id
    WHERE s.id = NEW.annulment_sale_id AND cs.status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot insert records into a closed session');
END;

-- Track this migration as applied.
INSERT OR IGNORE INTO schema_migrations (version) VALUES (1);
