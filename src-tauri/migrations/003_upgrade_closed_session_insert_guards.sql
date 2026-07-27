-- Add INSERT guards against closed cash sessions.
--
-- Version 1 and 2 protected existing rows from UPDATE/DELETE once a session was
-- closed, but they did not prevent new sales, payments, movements, drafts,
-- returns, or payment_reversals from being inserted against a closed session.
-- This migration closes that hole without altering prior migration history.

DROP TRIGGER IF EXISTS trg_guard_closed_session_sales_insert;
DROP TRIGGER IF EXISTS trg_guard_closed_session_payments_insert;
DROP TRIGGER IF EXISTS trg_guard_closed_session_movements_insert;
DROP TRIGGER IF EXISTS trg_guard_closed_session_drafts_insert;
DROP TRIGGER IF EXISTS trg_guard_closed_session_returns_insert;
DROP TRIGGER IF EXISTS trg_guard_closed_session_payment_reversals_insert;

CREATE TRIGGER trg_guard_closed_session_sales_insert
BEFORE INSERT ON sales
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot insert records into a closed session');
END;

CREATE TRIGGER trg_guard_closed_session_payments_insert
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

CREATE TRIGGER trg_guard_closed_session_movements_insert
BEFORE INSERT ON movements
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot insert records into a closed session');
END;

CREATE TRIGGER trg_guard_closed_session_drafts_insert
BEFORE INSERT ON drafts
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot insert records into a closed session');
END;

CREATE TRIGGER trg_guard_closed_session_returns_insert
BEFORE INSERT ON returns
FOR EACH ROW
WHEN EXISTS (
    SELECT 1 FROM cash_sessions WHERE id = NEW.session_id AND status = 'closed'
)
BEGIN
    SELECT RAISE(ABORT, 'cannot insert records into a closed session');
END;

CREATE TRIGGER trg_guard_closed_session_payment_reversals_insert
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

-- Record this migration atomically with the schema changes above.
INSERT OR REPLACE INTO schema_migrations (version) VALUES (3);
