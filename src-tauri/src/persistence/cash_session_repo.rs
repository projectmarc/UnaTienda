use deadpool_sqlite::Pool;
use rusqlite::OptionalExtension;

use crate::domain::cash_session::{CashSession, SessionStatus};
use crate::domain::error::AppError;

/// Row shape returned by session queries.
#[derive(Debug, Clone)]
pub struct CashSessionRow {
    pub id: String,
    pub status: String,
    pub opening_amount: i64,
    pub counted_cash: Option<i64>,
    pub difference: Option<i64>,
    pub opened_at: String,
    pub closed_at: Option<String>,
}

impl CashSessionRow {
    pub fn to_domain(&self) -> CashSession {
        CashSession {
            id: self.id.clone(),
            status: match self.status.as_str() {
                "open" => SessionStatus::Open,
                _ => SessionStatus::Closed,
            },
            opening_amount: self.opening_amount,
            counted_cash: self.counted_cash,
            difference: self.difference,
            opened_at: self.opened_at.clone(),
            closed_at: self.closed_at.clone(),
        }
    }
}

/// Insert an open cash session.
///
/// The partial unique index `idx_one_open_session` in SQLite guarantees
/// at most one open session. If a second open session is attempted, the
/// database returns a constraint violation which surfaces as `AppError::Conflict`.
pub async fn insert_open(pool: &Pool, session: &CashSession) -> Result<(), AppError> {
    let id = session.id.clone();
    let opening_amount = session.opening_amount;
    let opened_at = session.opened_at.clone();

    pool.get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(move |conn| {
            conn.execute(
                "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, "open", opening_amount, opened_at],
            )
            .map_err(|e| {
                if e.to_string().contains("idx_one_open_session") {
                    AppError::conflict("a session is already open")
                } else {
                    AppError::Database(e.to_string())
                }
            })?;
            Ok::<_, AppError>(())
        })
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
}

/// Query the current open session, if any.
pub async fn get_open(pool: &Pool) -> Result<Option<CashSession>, AppError> {
    let row: Option<CashSessionRow> = pool
        .get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(|conn| {
            conn.query_row(
                "SELECT id, status, opening_amount, counted_cash, difference, opened_at, closed_at FROM cash_sessions WHERE status = 'open' LIMIT 1",
                [],
                |row| {
                    Ok(CashSessionRow {
                        id: row.get(0)?,
                        status: row.get(1)?,
                        opening_amount: row.get(2)?,
                        counted_cash: row.get(3)?,
                        difference: row.get(4)?,
                        opened_at: row.get(5)?,
                        closed_at: row.get(6)?,
                    })
                },
            )
            .optional()
        })
        .await
        .map_err(|e| AppError::Database(e.to_string()))??;

    Ok(row.map(|r| r.to_domain()))
}

/// Snapshot the session data and close it atomically.
///
/// Updates the session row with `counted_cash`, `difference`, `closed_at`,
/// and transitions `status` to `closed`.
pub async fn snapshot_and_close(pool: &Pool, session: &CashSession) -> Result<(), AppError> {
    let id = session.id.clone();
    let counted_cash = session.counted_cash.unwrap_or(0);
    let difference = session.difference.unwrap_or(0);
    let closed_at = session.closed_at.clone().unwrap_or_default();

    pool.get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(move |conn| {
            let rows_changed = conn.execute(
                "UPDATE cash_sessions SET status = 'closed', counted_cash = ?2, difference = ?3, closed_at = ?4 WHERE id = ?1 AND status = 'open'",
                rusqlite::params![id, counted_cash, difference, closed_at],
            )?;
            if rows_changed == 0 {
                return Err(AppError::SessionClosed);
            }
            Ok::<_, AppError>(())
        })
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
}

/// Query the expected cash for an open session.
///
/// Expected cash = opening_amount
///   + cash payments for sales in this session
///   − cash payment_reversals attributed to this session
///     (via the return's session or the annulment sale's session,
///      NOT the original payment's sale session)
///   + income movements
///   − expense movements
///   − withdrawal movements
///
/// Returns 0 if no session is open or no aggregations exist.
pub async fn get_expected_cash(pool: &Pool, session_id: &str) -> Result<i64, AppError> {
    let sid = session_id.to_string();
    let result: Option<i64> = pool
        .get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(move |conn| {
            conn.query_row(
                r#"
                SELECT
                    COALESCE(cs.opening_amount, 0)
                    + COALESCE((
                        SELECT SUM(p.amount) FROM payments p
                        JOIN sales s ON s.id = p.sale_id
                        WHERE s.session_id = cs.id AND p.method = 'cash'
                    ), 0)
                    - COALESCE((
                        SELECT SUM(pr.amount) FROM payment_reversals pr
                        JOIN payments p ON p.id = pr.payment_id
                        WHERE p.method = 'cash'
                          AND (
                            (pr.return_id IS NOT NULL AND EXISTS (
                                SELECT 1 FROM returns r
                                WHERE r.id = pr.return_id AND r.session_id = cs.id
                            ))
                            OR
                            (pr.annulment_sale_id IS NOT NULL AND EXISTS (
                                SELECT 1 FROM sales sa
                                WHERE sa.id = pr.annulment_sale_id AND sa.session_id = cs.id
                            ))
                          )
                    ), 0)
                    + COALESCE((
                        SELECT SUM(CASE WHEN m.kind = 'income' THEN m.amount
                                        WHEN m.kind = 'expense' THEN -m.amount
                                        WHEN m.kind = 'withdrawal' THEN -m.amount
                                        ELSE 0 END)
                        FROM movements m
                        WHERE m.session_id = cs.id
                    ), 0)
                FROM cash_sessions cs
                WHERE cs.id = ?1 AND cs.status = 'open'
                "#,
                [&sid],
                |row| {
                    let val: i64 = row.get(0)?;
                    Ok(val)
                },
            )
            .optional()
        })
        .await
        .map_err(|e| AppError::Database(e.to_string()))??;

    Ok(result.unwrap_or(0))
}

/// Count the number of suspended drafts for a session.
///
/// Used by the close guard to reject closure when drafts remain.
pub async fn get_draft_count(pool: &Pool, session_id: &str) -> Result<i64, AppError> {
    let sid = session_id.to_string();
    let count = pool
        .get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM drafts WHERE session_id = ?1",
                [&sid],
                |row| {
                    let c: i64 = row.get(0)?;
                    Ok(c)
                },
            )
        })
        .await
        .map_err(|e| AppError::Database(e.to_string()))??;

    Ok(count)
}
