-- Upgrade existing version-1 databases to the complete closed-session immutability model.
--
-- Version 1 shipped same-name immutability triggers that either did not exist or
-- only guarded a subset of columns (for example, the `sales` update trigger listed
-- specific columns and omitted `annulment_reason`, and `sales`/`cash_sessions`/
-- `drafts` had no DELETE guards). Because SQLite's `CREATE TRIGGER IF NOT EXISTS`
-- never replaces an existing trigger, those legacy definitions survive a plain
-- re-run of migration 1. This migration explicitly drops each legacy same-name
-- trigger and recreates the complete guards.

DROP TRIGGER IF EXISTS trg_guard_closed_session_cash_sessions_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_cash_sessions_delete;
DROP TRIGGER IF EXISTS trg_guard_closed_session_sales_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_sales_delete;
DROP TRIGGER IF EXISTS trg_guard_closed_session_payments_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_payments_delete;
DROP TRIGGER IF EXISTS trg_guard_closed_session_movements_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_movements_delete;
DROP TRIGGER IF EXISTS trg_guard_closed_session_drafts_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_drafts_delete;
DROP TRIGGER IF EXISTS trg_guard_closed_session_returns_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_returns_delete;
DROP TRIGGER IF EXISTS trg_guard_closed_session_payment_reversals_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_payment_reversals_delete;

CREATE TRIGGER trg_guard_closed_session_cash_sessions_update
BEFORE UPDATE ON cash_sessions
FOR EACH ROW
WHEN OLD.status = 'closed'
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_cash_sessions_delete
BEFORE DELETE ON cash_sessions
FOR EACH ROW
WHEN OLD.status = 'closed'
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_sales_update
BEFORE UPDATE ON sales
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
OR EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_sales_delete
BEFORE DELETE ON sales
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_payments_update
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

CREATE TRIGGER trg_guard_closed_session_payments_delete
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

CREATE TRIGGER trg_guard_closed_session_movements_update
BEFORE UPDATE ON movements
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
OR EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_movements_delete
BEFORE DELETE ON movements
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_drafts_update
BEFORE UPDATE ON drafts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
OR EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_drafts_delete
BEFORE DELETE ON drafts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_returns_update
BEFORE UPDATE ON returns
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
OR EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_returns_delete
BEFORE DELETE ON returns
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = OLD.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_payment_reversals_update
BEFORE UPDATE ON payment_reversals
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM returns r
    JOIN cash_sessions cs ON cs.id = r.session_id
    WHERE r.id = OLD.return_id AND cs.status = 'closed'
)
OR EXISTS (
    SELECT 1 FROM sales s
    JOIN cash_sessions cs ON cs.id = s.session_id
    WHERE s.id = OLD.annulment_sale_id AND cs.status = 'closed'
)
OR EXISTS (
    SELECT 1 FROM returns r
    JOIN cash_sessions cs ON cs.id = r.session_id
    WHERE r.id = NEW.return_id AND cs.status = 'closed'
)
OR EXISTS (
    SELECT 1 FROM sales s
    JOIN cash_sessions cs ON cs.id = s.session_id
    WHERE s.id = NEW.annulment_sale_id AND cs.status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

CREATE TRIGGER trg_guard_closed_session_payment_reversals_delete
BEFORE DELETE ON payment_reversals
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM returns r
    JOIN cash_sessions cs ON cs.id = r.session_id
    WHERE r.id = OLD.return_id AND cs.status = 'closed'
)
OR EXISTS (
    SELECT 1 FROM sales s
    JOIN cash_sessions cs ON cs.id = s.session_id
    WHERE s.id = OLD.annulment_sale_id AND cs.status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'closed session records are immutable');
END;

-- Record this migration atomically with the schema changes above.
INSERT OR REPLACE INTO schema_migrations (version) VALUES (2);
