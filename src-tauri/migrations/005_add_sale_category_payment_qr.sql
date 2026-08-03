-- Add category to sales, QR payment method, and recipient_account to payments.

-- Add category column to sales with the three allowed values.
-- Existing rows default to 'mixto' since we cannot know their original category.
ALTER TABLE sales ADD COLUMN category TEXT NOT NULL DEFAULT 'mixto'
    CHECK (category IN ('ropa', 'libreria', 'mixto'));

-- The existing payments CHECK constraint only allows cash/transfer/debit/credit.
-- SQLite does not support ALTER COLUMN, so we must rebuild the table.
-- Foreign keys are disabled at the connection level by run_migrations().

-- Step 1: Create the new payments table with the expanded constraint.
CREATE TABLE IF NOT EXISTS payments_new (
    id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL,
    method TEXT NOT NULL CHECK (method IN ('cash', 'transfer', 'debit', 'credit', 'qr')),
    amount INTEGER NOT NULL,
    account TEXT,
    recipient_account TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY (sale_id) REFERENCES sales(id) ON DELETE RESTRICT
);

-- Step 2: Copy existing data.
INSERT INTO payments_new (id, sale_id, method, amount, account, created_at)
    SELECT id, sale_id, method, amount, account, created_at FROM payments;

-- Step 3: Drop the old table and rename.
DROP TABLE payments;
ALTER TABLE payments_new RENAME TO payments;

-- Step 4: Recreate the index.
CREATE INDEX IF NOT EXISTS idx_payments_sale_id ON payments(sale_id);

-- Step 5: Recreate the closed-session immutability triggers for payments.
CREATE TRIGGER IF NOT EXISTS trg_guard_closed_session_payments_update
BEFORE UPDATE ON payments
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions cs
    JOIN sales s ON s.session_id = cs.id
    WHERE s.id = OLD.sale_id AND cs.status = 'closed'
)
OR EXISTS (
    SELECT 1 FROM cash_sessions cs
    JOIN sales s ON s.session_id = cs.id
    WHERE s.id = NEW.sale_id AND cs.status = 'closed'
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

-- Track this migration as applied.
INSERT OR IGNORE INTO schema_migrations (version) VALUES (5);
