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

#[derive(Debug, Clone)]
pub struct SessionPaymentRow {
    pub method: String,
    pub amount: i64,
    pub recipient_account: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionOperationRow {
    pub id: String,
    pub occurred_at: String,
    pub operation_type: String,
    pub amount: i64,
    pub detail: Option<String>,
    pub category: Option<String>,
    pub payments: Vec<SessionPaymentRow>,
    pub status: Option<String>,
    pub annulment_reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PaymentTotalsRow {
    pub cash: i64,
    pub transfer: i64,
    pub debit: i64,
    pub credit: i64,
    pub qr: i64,
}

#[derive(Debug, Clone)]
pub struct TransferAccountTotalRow {
    pub recipient_account: String,
    pub amount: i64,
}

#[derive(Debug, Clone)]
pub struct SessionHistoryRows {
    pub operations: Vec<SessionOperationRow>,
    pub payment_totals: PaymentTotalsRow,
    pub transfer_totals: Vec<TransferAccountTotalRow>,
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

/// Query closed sessions in reverse chronological order.
pub async fn list_closed(pool: &Pool) -> Result<Vec<CashSessionRow>, AppError> {
    pool.get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(|conn| {
            let mut statement = conn.prepare(
                "SELECT id, status, opening_amount, counted_cash, difference, opened_at, closed_at FROM cash_sessions WHERE status = 'closed' ORDER BY closed_at DESC, id DESC",
            )?;
            let rows = statement
                .query_map([], |row| {
                    Ok(CashSessionRow {
                        id: row.get(0)?,
                        status: row.get(1)?,
                        opening_amount: row.get(2)?,
                        counted_cash: row.get(3)?,
                        difference: row.get(4)?,
                        opened_at: row.get(5)?,
                        closed_at: row.get(6)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, rusqlite::Error>(rows)
        })
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .map_err(AppError::from)
}

/// Query one closed session by ID.
pub async fn get_closed(pool: &Pool, session_id: &str) -> Result<Option<CashSessionRow>, AppError> {
    let sid = session_id.to_string();
    pool.get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(move |conn| {
            conn.query_row(
                "SELECT id, status, opening_amount, counted_cash, difference, opened_at, closed_at FROM cash_sessions WHERE id = ?1 AND status = 'closed'",
                [&sid],
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
        .map_err(|e| AppError::Database(e.to_string()))?
        .map_err(AppError::from)
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
async fn query_expected_cash(
    pool: &Pool,
    session_id: &str,
    only_open: bool,
) -> Result<i64, AppError> {
    let sid = session_id.to_string();
    let status_condition = if only_open {
        " AND cs.status = 'open'"
    } else {
        ""
    };
    let query = format!(
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
                WHERE cs.id = ?1{}
                "#,
        status_condition
    );
    let result: Option<i64> = pool
        .get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(move |conn| {
            conn.query_row(&query, [&sid], |row| {
                let val: i64 = row.get(0)?;
                Ok(val)
            })
            .optional()
        })
        .await
        .map_err(|e| AppError::Database(e.to_string()))??;

    Ok(result.unwrap_or(0))
}

pub async fn get_expected_cash(pool: &Pool, session_id: &str) -> Result<i64, AppError> {
    query_expected_cash(pool, session_id, true).await
}

/// Query expected cash for an open or closed session.
pub async fn get_expected_cash_for_session(pool: &Pool, session_id: &str) -> Result<i64, AppError> {
    query_expected_cash(pool, session_id, false).await
}

/// Query the chronological operations and net payment totals for a session.
///
/// Sale rows retain their original payment breakdown even after annulment so
/// the history remains an audit trail. Payment totals subtract reversals caused
/// by annulments in this session, which reflects the net operation result.
pub async fn get_history(pool: &Pool, session_id: &str) -> Result<SessionHistoryRows, AppError> {
    let sid = session_id.to_string();
    pool.get()
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .interact(move |conn| {
            let sales: Vec<(
                String,
                i64,
                String,
                Option<String>,
                String,
                Option<String>,
                String,
            )> = {
                let mut statement = conn.prepare(
                    "SELECT id, total_amount, category, details, status, annulment_reason, created_at FROM sales WHERE session_id = ?1 ORDER BY created_at ASC, id ASC",
                )?;
                let rows = statement
                    .query_map([&sid], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };

            let mut operations = Vec::with_capacity(sales.len());
            for (
                id,
                amount,
                category,
                detail,
                status,
                annulment_reason,
                occurred_at,
            ) in sales
            {
                let payments = {
                    let mut statement = conn.prepare(
                        "SELECT method, amount, recipient_account FROM payments WHERE sale_id = ?1 ORDER BY created_at ASC, id ASC",
                    )?;
                    let rows = statement
                        .query_map([&id], |row| {
                            Ok(SessionPaymentRow {
                                method: row.get(0)?,
                                amount: row.get(1)?,
                                recipient_account: row.get(2)?,
                            })
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    rows
                };

                operations.push(SessionOperationRow {
                    id,
                    occurred_at,
                    operation_type: "sale".to_string(),
                    amount,
                    detail,
                    category: Some(category),
                    payments,
                    status: Some(status),
                    annulment_reason,
                });
            }

            let movements: Vec<(String, i64, Option<String>, String)> = {
                let mut statement = conn.prepare(
                    "SELECT id, amount, note, created_at FROM movements WHERE session_id = ?1 ORDER BY created_at ASC, id ASC",
                )?;
                let rows = statement
                    .query_map([&sid], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };

            for (id, amount, detail, occurred_at) in movements {
                operations.push(SessionOperationRow {
                    id,
                    occurred_at,
                    operation_type: "cash_out".to_string(),
                    amount,
                    detail,
                    category: None,
                    payments: Vec::new(),
                    status: None,
                    annulment_reason: None,
                });
            }

            operations.sort_by(|left, right| {
                left.occurred_at
                    .cmp(&right.occurred_at)
                    .then_with(|| left.id.cmp(&right.id))
            });

            let mut payment_totals = PaymentTotalsRow::default();
            let mut statement = conn.prepare(
                r#"
                SELECT p.method,
                       SUM(
                           p.amount - COALESCE((
                               SELECT SUM(pr.amount)
                               FROM payment_reversals pr
                               WHERE pr.payment_id = p.id
                                 AND pr.annulment_sale_id IN (
                                     SELECT id FROM sales WHERE session_id = ?1
                                 )
                           ), 0)
                       )
                FROM payments p
                JOIN sales s ON s.id = p.sale_id
                WHERE s.session_id = ?1
                GROUP BY p.method
                "#,
            )?;
            let totals = statement.query_map([&sid], |row| {
                let method: String = row.get(0)?;
                let amount: i64 = row.get(1)?;
                Ok((method, amount))
            })?;
            for total in totals {
                let (method, amount) = total?;
                match method.as_str() {
                    "cash" => payment_totals.cash = amount,
                    "transfer" => payment_totals.transfer = amount,
                    "debit" => payment_totals.debit = amount,
                    "credit" => payment_totals.credit = amount,
                    "qr" => payment_totals.qr = amount,
                    _ => {}
                }
            }

            let mut statement = conn.prepare(
                r#"
                WITH net_transfers AS (
                    SELECT p.recipient_account,
                           p.amount - COALESCE((
                               SELECT SUM(pr.amount)
                               FROM payment_reversals pr
                               WHERE pr.payment_id = p.id
                                 AND pr.annulment_sale_id IN (
                                     SELECT id FROM sales WHERE session_id = ?1
                                 )
                           ), 0) AS amount
                    FROM payments p
                    JOIN sales s ON s.id = p.sale_id
                    WHERE s.session_id = ?1
                      AND p.method = 'transfer'
                      AND p.recipient_account IS NOT NULL
                )
                SELECT recipient_account, SUM(amount)
                FROM net_transfers
                GROUP BY recipient_account
                HAVING SUM(amount) > 0
                ORDER BY recipient_account ASC
                "#,
            )?;
            let transfer_totals = statement
                .query_map([&sid], |row| {
                    Ok(TransferAccountTotalRow {
                        recipient_account: row.get(0)?,
                        amount: row.get(1)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok::<_, rusqlite::Error>(SessionHistoryRows {
                operations,
                payment_totals,
                transfer_totals,
            })
        })
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .map_err(AppError::from)
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
