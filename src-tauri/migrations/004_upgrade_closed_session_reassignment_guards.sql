-- Prevent open-session records from being reassigned into closed sessions.
--
-- Versions 1-3 checked only each row's OLD owner during UPDATE. An open-session
-- row could therefore move to a closed session (directly through session_id or
-- indirectly through sale/source foreign keys). Replace those guards and use
-- source-session ownership for payment reversals.

DROP TRIGGER IF EXISTS trg_guard_closed_session_sales_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_payments_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_movements_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_drafts_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_returns_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_payment_reversals_update;
DROP TRIGGER IF EXISTS trg_guard_closed_session_payment_reversals_delete;

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

-- Record this migration atomically with the trigger replacements above.
INSERT OR REPLACE INTO schema_migrations (version) VALUES (4);
