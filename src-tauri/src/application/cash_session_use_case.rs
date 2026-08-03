use deadpool_sqlite::Pool;
use serde::{Deserialize, Serialize};

use crate::domain::cash_session::CashSession;
use crate::domain::error::AppError;
use crate::domain::Money;
use crate::persistence::cash_session_repo;
use crate::persistence::idempotency::with_idempotency;

/// Response returned after opening a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionResponse {
    pub session_id: String,
    pub opening_amount: String,
}

/// Response returned after closing a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedSessionResponse {
    pub session_id: String,
    pub opening_amount: String,
    pub expected_cash: String,
    pub counted_cash: String,
    pub difference: String,
}

/// Reconciliation view for the current open session.
///
/// When `counted_cash` is provided, `difference` is computed exactly using
/// integer arithmetic (no floating-point). When omitted, both are `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationView {
    pub session_id: String,
    pub opening_amount: String,
    pub expected_cash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counted_cash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difference: Option<String>,
}

/// A payment part shown in the open-session history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPayment {
    pub method: String,
    pub amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_account: Option<String>,
}

/// A sale or cash-out shown in the open-session history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOperation {
    pub id: String,
    pub occurred_at: String,
    pub operation_type: String,
    pub amount: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub payments: Vec<SessionPayment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annulment_reason: Option<String>,
}

/// Net totals by payment method for the open session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentTotals {
    pub cash: String,
    pub transfer: String,
    pub debit: String,
    pub credit: String,
    pub qr: String,
}

/// Net transfer totals grouped by recipient account for the open session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferAccountTotal {
    pub recipient_account: String,
    pub amount: String,
}

/// History, payment totals, and expected cash for the open session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHistoryResponse {
    pub operations: Vec<SessionOperation>,
    pub payment_totals: PaymentTotals,
    pub transfer_totals: Vec<TransferAccountTotal>,
    pub expected_cash: String,
}

/// Reconciliation summary for a closed session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedSessionSummary {
    pub session_id: String,
    pub opened_at: String,
    pub closed_at: String,
    pub opening_amount: String,
    pub expected_cash: String,
    pub counted_cash: String,
    pub difference: String,
}

/// Read-only operation history and reconciliation for a closed session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedSessionDetailResponse {
    pub session_id: String,
    pub opened_at: String,
    pub closed_at: String,
    pub opening_amount: String,
    pub expected_cash: String,
    pub counted_cash: String,
    pub difference: String,
    pub operations: Vec<SessionOperation>,
    pub payment_totals: PaymentTotals,
    pub transfer_totals: Vec<TransferAccountTotal>,
}

/// Open a new cash session.
///
/// Enforces the single-open-session invariant (the database partial unique
/// index provides the hard guard). Uses an idempotency key so the same
/// request replays the stored response.
pub async fn open_session(
    pool: &Pool,
    opening_amount: i64,
    idempotency_key: &str,
) -> Result<OpenSessionResponse, AppError> {
    let amount = opening_amount;
    let request_hash = format!("open_session:{}", amount);

    with_idempotency(pool, idempotency_key, "open_session", &request_hash, move |tx| {
        // Verify no open session exists.
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM cash_sessions WHERE status = 'open' LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        if existing.is_some() {
            return Err(AppError::conflict("a session is already open"));
        }

        let session = CashSession::open(uuid::Uuid::new_v4().to_string(), amount)?;

        tx.execute(
            "INSERT INTO cash_sessions (id, status, opening_amount, opened_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session.id, session.status.as_str(), session.opening_amount, session.opened_at],
        )?;

        Ok(OpenSessionResponse {
            session_id: session.id,
            opening_amount: Money::from_cents(session.opening_amount).to_string(),
        })
    })
    .await
}

/// Close the current open session.
///
/// Queries expected cash and draft count, then delegates to the domain
/// `close` method for invariant enforcement. The close and idempotency
/// commit happen atomically in one transaction.
pub async fn close_session(
    pool: &Pool,
    counted_cash: i64,
    idempotency_key: &str,
) -> Result<ClosedSessionResponse, AppError> {
    let cash = counted_cash;
    let request_hash = format!("close_session:{}", cash);

    with_idempotency(pool, idempotency_key, "close_session", &request_hash, move |tx| {
        // Query the open session within the transaction.
        let session_row: Option<(String, i64)> = tx
            .query_row(
                "SELECT id, opening_amount FROM cash_sessions WHERE status = 'open' LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let (session_id, opening_amount) =
            session_row.ok_or(AppError::NoOpenSession)?;

        // Expected cash aggregation within the transaction.
        // JD2-A-001 fix: attribute reversal amounts to the return/annulment session,
        // not the original payment's sale session.
        // JD2-A-002 fix: propagate query errors instead of swallowing with unwrap_or.
        let expected_cash: i64 = tx
            .query_row(
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
                WHERE cs.id = ?1
                "#,
                [&session_id],
                |row| {
                    let val: i64 = row.get(0)?;
                    Ok(val)
                },
            )?;

        // Draft count within the transaction.
        // JD2-A-002 fix: propagate query errors instead of swallowing with unwrap_or.
        let draft_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM drafts WHERE session_id = ?1",
                [&session_id],
                |row| {
                    let c: i64 = row.get(0)?;
                    Ok(c)
                },
            )?;

        // Domain invariant enforcement.
        let mut session = CashSession {
            id: session_id.clone(),
            status: crate::domain::cash_session::SessionStatus::Open,
            opening_amount,
            counted_cash: None,
            difference: None,
            opened_at: String::new(),
            closed_at: None,
        };
        session.close(expected_cash, cash, draft_count)?;

        let closed_at = session.closed_at.clone().unwrap_or_default();
        let difference = session.difference.unwrap_or(0);

        tx.execute(
            "UPDATE cash_sessions SET status = 'closed', counted_cash = ?2, difference = ?3, closed_at = ?4 WHERE id = ?1 AND status = 'open'",
            rusqlite::params![session_id, cash, difference, closed_at],
        )?;

        Ok(ClosedSessionResponse {
            session_id,
            opening_amount: Money::from_cents(opening_amount).to_string(),
            expected_cash: Money::from_cents(expected_cash).to_string(),
            counted_cash: Money::from_cents(cash).to_string(),
            difference: Money::from_cents(difference).to_string(),
        })
    })
    .await
}

/// Get the reconciliation view for the current open session.
///
/// This is a read-only query and does not require an idempotency key.
///
/// When `counted_cash` is provided (as a decimal string), the response
/// includes the exact difference computed via `Money::checked_sub` — no
/// floating-point arithmetic. When omitted, `counted_cash` and `difference`
/// are `None` in the response.
pub async fn get_reconciliation(
    pool: &Pool,
    counted_cash: Option<&str>,
) -> Result<ReconciliationView, AppError> {
    let session = cash_session_repo::get_open(pool)
        .await?
        .ok_or(AppError::NoOpenSession)?;

    let expected_cash = cash_session_repo::get_expected_cash(pool, &session.id).await?;

    let (counted_str, difference_str) = match counted_cash {
        Some(cc_str) => {
            let cc: Money = cc_str.parse()?;
            let expected = Money::from_cents(expected_cash);
            let diff = cc.checked_sub(&expected)?;
            (Some(cc.to_string()), Some(diff.to_string()))
        }
        None => (None, None),
    };

    Ok(ReconciliationView {
        session_id: session.id,
        opening_amount: Money::from_cents(session.opening_amount).to_string(),
        expected_cash: Money::from_cents(expected_cash).to_string(),
        counted_cash: counted_str,
        difference: difference_str,
    })
}

fn map_history_rows(
    history: cash_session_repo::SessionHistoryRows,
) -> (
    Vec<SessionOperation>,
    PaymentTotals,
    Vec<TransferAccountTotal>,
) {
    let cash_session_repo::SessionHistoryRows {
        operations,
        payment_totals,
        transfer_totals,
    } = history;

    (
        operations
            .into_iter()
            .map(|operation| SessionOperation {
                id: operation.id,
                occurred_at: operation.occurred_at,
                operation_type: operation.operation_type,
                amount: Money::from_cents(operation.amount).to_string(),
                detail: operation.detail,
                category: operation.category,
                payments: operation
                    .payments
                    .into_iter()
                    .map(|payment| SessionPayment {
                        method: payment.method,
                        amount: Money::from_cents(payment.amount).to_string(),
                        recipient_account: payment.recipient_account,
                    })
                    .collect(),
                status: operation.status,
                annulment_reason: operation.annulment_reason,
            })
            .collect(),
        PaymentTotals {
            cash: Money::from_cents(payment_totals.cash).to_string(),
            transfer: Money::from_cents(payment_totals.transfer).to_string(),
            debit: Money::from_cents(payment_totals.debit).to_string(),
            credit: Money::from_cents(payment_totals.credit).to_string(),
            qr: Money::from_cents(payment_totals.qr).to_string(),
        },
        transfer_totals
            .into_iter()
            .map(|total| TransferAccountTotal {
                recipient_account: total.recipient_account,
                amount: Money::from_cents(total.amount).to_string(),
            })
            .collect(),
    )
}

fn closed_session_summary(
    row: cash_session_repo::CashSessionRow,
    expected_cash: i64,
) -> Result<ClosedSessionSummary, AppError> {
    let counted_cash = row
        .counted_cash
        .ok_or_else(|| AppError::Database("closed session is missing counted cash".to_string()))?;
    let difference = row
        .difference
        .ok_or_else(|| AppError::Database("closed session is missing difference".to_string()))?;
    let closed_at = row.closed_at.ok_or_else(|| {
        AppError::Database("closed session is missing closed timestamp".to_string())
    })?;

    Ok(ClosedSessionSummary {
        session_id: row.id,
        opened_at: row.opened_at,
        closed_at,
        opening_amount: Money::from_cents(row.opening_amount).to_string(),
        expected_cash: Money::from_cents(expected_cash).to_string(),
        counted_cash: Money::from_cents(counted_cash).to_string(),
        difference: Money::from_cents(difference).to_string(),
    })
}

/// Get the chronological operations and current net totals for the open session.
pub async fn get_session_history(pool: &Pool) -> Result<SessionHistoryResponse, AppError> {
    let session = cash_session_repo::get_open(pool)
        .await?
        .ok_or(AppError::NoOpenSession)?;
    let history = cash_session_repo::get_history(pool, &session.id).await?;
    let expected_cash = cash_session_repo::get_expected_cash(pool, &session.id).await?;

    let (operations, payment_totals, transfer_totals) = map_history_rows(history);

    Ok(SessionHistoryResponse {
        operations,
        payment_totals,
        transfer_totals,
        expected_cash: Money::from_cents(expected_cash).to_string(),
    })
}

/// List all closed sessions in reverse chronological order.
pub async fn list_closed_sessions(pool: &Pool) -> Result<Vec<ClosedSessionSummary>, AppError> {
    let rows = cash_session_repo::list_closed(pool).await?;
    let mut summaries = Vec::with_capacity(rows.len());

    for row in rows {
        let expected_cash = cash_session_repo::get_expected_cash_for_session(pool, &row.id).await?;
        summaries.push(closed_session_summary(row, expected_cash)?);
    }

    Ok(summaries)
}

/// Get the immutable operation history and final reconciliation of one closed session.
pub async fn get_closed_session_detail(
    pool: &Pool,
    session_id: &str,
) -> Result<ClosedSessionDetailResponse, AppError> {
    let row = cash_session_repo::get_closed(pool, session_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let expected_cash = cash_session_repo::get_expected_cash_for_session(pool, &row.id).await?;
    let summary = closed_session_summary(row, expected_cash)?;
    let history = cash_session_repo::get_history(pool, &summary.session_id).await?;
    let (operations, payment_totals, transfer_totals) = map_history_rows(history);

    Ok(ClosedSessionDetailResponse {
        session_id: summary.session_id,
        opened_at: summary.opened_at,
        closed_at: summary.closed_at,
        opening_amount: summary.opening_amount,
        expected_cash: summary.expected_cash,
        counted_cash: summary.counted_cash,
        difference: summary.difference,
        operations,
        payment_totals,
        transfer_totals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::sale_use_case::{
        record_cash_out, record_sale, PaymentPartRequest, RecordSaleRequest,
    };
    use crate::persistence::connection::{open_pool, run_migrations};
    use tempfile::TempDir;
    use uuid::Uuid;

    /// Helper: create a fresh temporary database with all migrations applied.
    async fn setup_db() -> (TempDir, Pool) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool, &db_path).await.unwrap();
        (dir, pool)
    }

    #[tokio::test]
    async fn open_session_succeeds() {
        let (_dir, pool) = setup_db().await;

        let resp = open_session(&pool, 5000, "key-open-1").await.unwrap();

        assert!(!resp.session_id.is_empty());
        assert_eq!(resp.opening_amount, "50.00");
    }

    #[tokio::test]
    async fn open_session_rejects_second_session() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 1000, "key-first").await.unwrap();

        let err = open_session(&pool, 2000, "key-second").await.unwrap_err();
        assert!(matches!(err, AppError::Conflict { .. }));
    }

    #[tokio::test]
    async fn open_session_rejects_negative_amount() {
        let (_dir, pool) = setup_db().await;

        let err = open_session(&pool, -100, "key-neg").await.unwrap_err();
        assert!(matches!(err, AppError::Validation { .. }));
    }

    #[tokio::test]
    async fn open_session_allows_zero_amount() {
        let (_dir, pool) = setup_db().await;

        let resp = open_session(&pool, 0, "key-zero").await.unwrap();
        assert_eq!(resp.opening_amount, "0.00");
    }

    #[tokio::test]
    async fn close_session_records_difference() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        // Counted 10500, expected 10000 (no sales/movements) => difference +500
        let resp = close_session(&pool, 10500, "key-close").await.unwrap();

        assert_eq!(resp.opening_amount, "100.00");
        assert_eq!(resp.expected_cash, "100.00");
        assert_eq!(resp.counted_cash, "105.00");
        assert_eq!(resp.difference, "5.00");
    }

    #[tokio::test]
    async fn close_session_with_zero_difference() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 5000, "key-open").await.unwrap();

        let resp = close_session(&pool, 5000, "key-close").await.unwrap();

        assert_eq!(resp.expected_cash, "50.00");
        assert_eq!(resp.counted_cash, "50.00");
        assert_eq!(resp.difference, "0.00");
    }

    #[tokio::test]
    async fn close_session_rejects_when_no_session_open() {
        let (_dir, pool) = setup_db().await;

        let err = close_session(&pool, 1000, "key-no-session")
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NoOpenSession));
    }

    #[tokio::test]
    async fn close_session_rejects_when_already_closed() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 1000, "key-open").await.unwrap();
        close_session(&pool, 1000, "key-close").await.unwrap();

        let err = close_session(&pool, 1000, "key-close-2").await.unwrap_err();
        assert!(matches!(err, AppError::NoOpenSession));
    }

    #[tokio::test]
    async fn close_session_blocks_when_drafts_remain() {
        let (_dir, pool) = setup_db().await;

        let open = open_session(&pool, 1000, "key-open").await.unwrap();

        // Insert a draft directly into the database to simulate a suspended draft.
        // The drafts table has no status column; a draft's presence means "suspended".
        pool.get()
            .await
            .unwrap()
            .interact(move |conn| {
                conn.execute(
                    "INSERT INTO drafts (id, session_id, created_at) VALUES (?1, ?2, '2024-01-01T00:00:00Z')",
                    rusqlite::params![Uuid::new_v4().to_string(), open.session_id],
                ).unwrap();
            })
            .await
            .unwrap();

        let err = close_session(&pool, 1000, "key-close").await.unwrap_err();
        assert!(matches!(err, AppError::DraftsBlockClose));
    }

    #[tokio::test]
    async fn get_reconciliation_returns_expected_cash_for_open_session() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let view = get_reconciliation(&pool, None).await.unwrap();

        assert_eq!(view.opening_amount, "100.00");
        assert_eq!(view.expected_cash, "100.00"); // No sales/movements yet
        assert!(view.counted_cash.is_none());
        assert!(view.difference.is_none());
    }

    #[tokio::test]
    async fn get_reconciliation_rejects_when_no_session_open() {
        let (_dir, pool) = setup_db().await;

        let err = get_reconciliation(&pool, None).await.unwrap_err();
        assert!(matches!(err, AppError::NoOpenSession));
    }

    #[tokio::test]
    async fn idempotency_replays_open_session() {
        let (_dir, pool) = setup_db().await;

        let first = open_session(&pool, 5000, "key-idem").await.unwrap();
        let second = open_session(&pool, 5000, "key-idem").await.unwrap();

        // Same key + same request hash => replay; session_id must be identical.
        assert_eq!(first.session_id, second.session_id);
        assert_eq!(first.opening_amount, second.opening_amount);
    }

    #[tokio::test]
    async fn idempotency_replays_close_session() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 5000, "key-open").await.unwrap();

        let first = close_session(&pool, 5000, "key-close").await.unwrap();
        let second = close_session(&pool, 5000, "key-close").await.unwrap();

        assert_eq!(first.session_id, second.session_id);
        assert_eq!(first.difference, second.difference);
    }

    #[tokio::test]
    async fn expected_cash_includes_movements() {
        let (_dir, pool) = setup_db().await;

        let open = open_session(&pool, 10000, "key-open").await.unwrap();

        // Insert an income movement and an expense movement directly.
        pool.get()
            .await
            .unwrap()
            .interact(move |conn| {
                conn.execute(
                    "INSERT INTO movements (id, session_id, kind, amount, created_at) VALUES (?1, ?2, 'income', 3000, '2024-01-01T00:00:00Z')",
                    rusqlite::params![Uuid::new_v4().to_string(), open.session_id],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO movements (id, session_id, kind, amount, created_at) VALUES (?1, ?2, 'expense', 500, '2024-01-01T00:00:00Z')",
                    rusqlite::params![Uuid::new_v4().to_string(), open.session_id],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO movements (id, session_id, kind, amount, created_at) VALUES (?1, ?2, 'withdrawal', 1000, '2024-01-01T00:00:00Z')",
                    rusqlite::params![Uuid::new_v4().to_string(), open.session_id],
                )
                .unwrap();
            })
            .await
            .unwrap();

        // expected = 10000 opening + 3000 income - 500 expense - 1000 withdrawal = 11500
        let view = get_reconciliation(&pool, None).await.unwrap();
        assert_eq!(view.expected_cash, "115.00");

        // Close with counted = 11500 => zero difference
        let resp = close_session(&pool, 11500, "key-close").await.unwrap();
        assert_eq!(resp.expected_cash, "115.00");
        assert_eq!(resp.counted_cash, "115.00");
        assert_eq!(resp.difference, "0.00");
    }

    #[tokio::test]
    async fn close_session_negative_difference() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        // Counted 9000, expected 10000 => difference -1000
        let resp = close_session(&pool, 9000, "key-close").await.unwrap();

        assert_eq!(resp.expected_cash, "100.00");
        assert_eq!(resp.counted_cash, "90.00");
        assert_eq!(resp.difference, "-10.00");
    }

    #[tokio::test]
    async fn open_after_close_allows_new_session() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 1000, "key-open-1").await.unwrap();
        close_session(&pool, 1000, "key-close").await.unwrap();

        // After closing, a new session should be allowed.
        let resp = open_session(&pool, 2000, "key-open-2").await.unwrap();
        assert_eq!(resp.opening_amount, "20.00");
    }

    /// JD2-A-001 regression: a return-based reversal must reduce the expected cash
    /// of the session where the return occurred, NOT the session of the original sale.
    #[tokio::test]
    async fn reversal_attributed_to_return_session_not_payment_session() {
        let (_dir, pool) = setup_db().await;

        // Session A: open, make a sale with a cash payment, close.
        let open_a = open_session(&pool, 10000, "key-open-a").await.unwrap();
        let session_a_id = open_a.session_id.clone();

        pool.get()
            .await
            .unwrap()
            .interact(move |conn| {
                let sale_id = Uuid::new_v4().to_string();
                let payment_id = Uuid::new_v4().to_string();
                // Sale in session A.
                conn.execute(
                    "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, 5000, 'confirmed')",
                    rusqlite::params![sale_id, session_a_id],
                )
                .unwrap();
                // Cash payment for that sale.
                conn.execute(
                    "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, 'cash', 5000)",
                    rusqlite::params![payment_id, sale_id],
                )
                .unwrap();
            })
            .await
            .unwrap();

        // Session A expected = 10000 opening + 5000 cash payment = 15000
        let resp_a = close_session(&pool, 15000, "key-close-a").await.unwrap();
        assert_eq!(resp_a.expected_cash, "150.00");
        assert_eq!(resp_a.difference, "0.00");

        // Session B: open, create a return with a reversal for session A's payment.
        let open_b = open_session(&pool, 5000, "key-open-b").await.unwrap();
        let session_b_id = open_b.session_id.clone();

        pool.get()
            .await
            .unwrap()
            .interact(move |conn| {
                // Re-use the same sale_id/payment_id from session A (they exist in DB).
                // We need to look them up.
                let sale_id: String = conn
                    .query_row(
                        "SELECT id FROM sales WHERE session_id != ?1 LIMIT 1",
                        [&session_b_id],
                        |row| row.get(0),
                    )
                    .unwrap();
                let payment_id: String = conn
                    .query_row(
                        "SELECT id FROM payments WHERE sale_id = ?1 LIMIT 1",
                        [&sale_id],
                        |row| row.get(0),
                    )
                    .unwrap();

                let return_id = Uuid::new_v4().to_string();
                let reversal_id = Uuid::new_v4().to_string();
                // Return in session B (references session A's sale).
                conn.execute(
                    "INSERT INTO returns (id, session_id, sale_id, return_kind, amount) VALUES (?1, ?2, ?3, 'partial', 2000)",
                    rusqlite::params![return_id, session_b_id, sale_id],
                )
                .unwrap();
                // Reversal of 2000 linked to the return.
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, return_id) VALUES (?1, ?2, 2000, ?3)",
                    rusqlite::params![reversal_id, payment_id, return_id],
                )
                .unwrap();
            })
            .await
            .unwrap();

        // Session B expected = 5000 opening - 2000 reversal (attributed to return session) = 3000
        // Before JD2-A-001 fix, the reversal would have been attributed to session A,
        // and session B expected would incorrectly be 5000.
        let view_b = get_reconciliation(&pool, None).await.unwrap();
        assert_eq!(
            view_b.expected_cash, "30.00",
            "reversal must be deducted from the return session, not the payment session"
        );

        let resp_b = close_session(&pool, 3000, "key-close-b").await.unwrap();
        assert_eq!(resp_b.expected_cash, "30.00");
        assert_eq!(resp_b.difference, "0.00");
    }

    /// JD2-A-001 regression: an annulment-based reversal must reduce the expected cash
    /// of the session where the annulment sale occurred.
    #[tokio::test]
    async fn reversal_attributed_to_annulment_sale_session() {
        let (_dir, pool) = setup_db().await;

        // Session A: open, make a sale with a cash payment, close.
        let open_a = open_session(&pool, 10000, "key-open-a").await.unwrap();
        let session_a_id = open_a.session_id.clone();

        pool.get()
            .await
            .unwrap()
            .interact(move |conn| {
                let sale_id = Uuid::new_v4().to_string();
                let payment_id = Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO sales (id, session_id, total_amount, status) VALUES (?1, ?2, 5000, 'confirmed')",
                    rusqlite::params![sale_id, session_a_id],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO payments (id, sale_id, method, amount) VALUES (?1, ?2, 'cash', 5000)",
                    rusqlite::params![payment_id, sale_id],
                )
                .unwrap();
            })
            .await
            .unwrap();

        let resp_a = close_session(&pool, 15000, "key-close-a").await.unwrap();
        assert_eq!(resp_a.expected_cash, "150.00");

        // Session B: open, create an annulment sale that triggers a reversal.
        let open_b = open_session(&pool, 5000, "key-open-b").await.unwrap();
        let session_b_id = open_b.session_id.clone();

        pool.get()
            .await
            .unwrap()
            .interact(move |conn| {
                let sale_id: String = conn
                    .query_row(
                        "SELECT id FROM sales WHERE session_id != ?1 LIMIT 1",
                        [&session_b_id],
                        |row| row.get(0),
                    )
                    .unwrap();
                let payment_id: String = conn
                    .query_row(
                        "SELECT id FROM payments WHERE sale_id = ?1 LIMIT 1",
                        [&sale_id],
                        |row| row.get(0),
                    )
                    .unwrap();

                let annulment_sale_id = Uuid::new_v4().to_string();
                let reversal_id = Uuid::new_v4().to_string();
                // Annulment sale in session B.
                conn.execute(
                    "INSERT INTO sales (id, session_id, total_amount, status, annulment_reason) VALUES (?1, ?2, 5000, 'annulled', 'customer request')",
                    rusqlite::params![annulment_sale_id, session_b_id],
                )
                .unwrap();
                // Reversal linked to the annulment sale (XOR: no return_id).
                conn.execute(
                    "INSERT INTO payment_reversals (id, payment_id, amount, annulment_sale_id) VALUES (?1, ?2, 3000, ?3)",
                    rusqlite::params![reversal_id, payment_id, annulment_sale_id],
                )
                .unwrap();
            })
            .await
            .unwrap();

        // Session B expected = 5000 opening - 3000 annulment reversal = 2000
        let view_b = get_reconciliation(&pool, None).await.unwrap();
        assert_eq!(
            view_b.expected_cash, "20.00",
            "annulment reversal must be deducted from the annulment sale session"
        );

        let resp_b = close_session(&pool, 2000, "key-close-b").await.unwrap();
        assert_eq!(resp_b.expected_cash, "20.00");
        assert_eq!(resp_b.difference, "0.00");
    }

    /// JD2-A-003 regression: extreme Money values that would overflow in the
    /// difference calculation must reject the close with a VALIDATION error.
    #[tokio::test]
    async fn close_session_rejects_overflowing_difference() {
        let (_dir, pool) = setup_db().await;

        // Open with a positive opening amount so expected_cash > 0.
        open_session(&pool, 1, "key-open").await.unwrap();

        // i64::MIN counted - 1 expected = underflow below i64::MIN.
        let err = close_session(&pool, i64::MIN, "key-close")
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::Validation { .. }),
            "overflow must produce a validation error, got: {:?}",
            err
        );
    }

    // --- V2-VERIFY-001: Reconciliation with exact counted-cash/difference ---

    /// Reconciliation with counted cash returns the exact difference computed
    /// via integer arithmetic, not floating-point.
    #[tokio::test]
    async fn reconciliation_with_counted_cash_returns_exact_difference() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        // expected = 10000 opening, counted = 10500 => difference = +500
        let view = get_reconciliation(&pool, Some("105.00")).await.unwrap();

        assert_eq!(view.opening_amount, "100.00");
        assert_eq!(view.expected_cash, "100.00");
        assert_eq!(view.counted_cash, Some("105.00".to_string()));
        assert_eq!(view.difference, Some("5.00".to_string()));
    }

    /// Reconciliation with counted cash less than expected returns negative difference.
    #[tokio::test]
    async fn reconciliation_with_counted_cash_returns_negative_difference() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        // expected = 10000, counted = 9000 => difference = -1000
        let view = get_reconciliation(&pool, Some("90.00")).await.unwrap();

        assert_eq!(view.expected_cash, "100.00");
        assert_eq!(view.counted_cash, Some("90.00".to_string()));
        assert_eq!(view.difference, Some("-10.00".to_string()));
    }

    /// Reconciliation with zero counted cash returns negative of expected.
    #[tokio::test]
    async fn reconciliation_with_zero_counted_cash() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 5000, "key-open").await.unwrap();

        let view = get_reconciliation(&pool, Some("0.00")).await.unwrap();

        assert_eq!(view.expected_cash, "50.00");
        assert_eq!(view.counted_cash, Some("0.00".to_string()));
        assert_eq!(view.difference, Some("-50.00".to_string()));
    }

    /// Reconciliation without counted cash returns None for both fields.
    #[tokio::test]
    async fn reconciliation_without_counted_cash_returns_none() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let view = get_reconciliation(&pool, None).await.unwrap();

        assert_eq!(view.expected_cash, "100.00");
        assert!(view.counted_cash.is_none());
        assert!(view.difference.is_none());
    }

    /// Reconciliation rejects invalid counted cash format.
    #[tokio::test]
    async fn reconciliation_rejects_invalid_counted_cash() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let err = get_reconciliation(&pool, Some("not-money"))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation { .. }));
    }

    /// Reconciliation rejects counted cash that would overflow the difference.
    #[tokio::test]
    async fn reconciliation_rejects_overflowing_difference() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 1, "key-open").await.unwrap();

        // i64::MIN counted - 1 expected = underflow below i64::MIN.
        let err = get_reconciliation(&pool, Some("-92233720368547758.09"))
            .await
            .unwrap_err();
        assert!(
            matches!(err, AppError::Validation { .. }),
            "overflow in reconciliation difference must produce a validation error, got: {:?}",
            err
        );
    }

    /// Reconciliation with counted cash equal to expected returns zero difference.
    #[tokio::test]
    async fn reconciliation_with_counted_equal_to_expected_returns_zero() {
        let (_dir, pool) = setup_db().await;

        open_session(&pool, 10000, "key-open").await.unwrap();

        let view = get_reconciliation(&pool, Some("100.00")).await.unwrap();

        assert_eq!(view.expected_cash, "100.00");
        assert_eq!(view.counted_cash, Some("100.00".to_string()));
        assert_eq!(view.difference, Some("0.00".to_string()));
    }

    #[tokio::test]
    async fn session_history_orders_operations_and_exposes_details_and_totals() {
        let (_dir, pool) = setup_db().await;
        open_session(&pool, 10000, "key-open").await.unwrap();

        let sale = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "120.00".to_string(),
                category: "mixto".to_string(),
                detail: Some("campera y cuaderno".to_string()),
                payments: vec![
                    PaymentPartRequest {
                        method: "cash".to_string(),
                        amount: "70.00".to_string(),
                        recipient_account: None,
                    },
                    PaymentPartRequest {
                        method: "transfer".to_string(),
                        amount: "50.00".to_string(),
                        recipient_account: Some("Juan".to_string()),
                    },
                ],
            },
            "key-sale-history",
        )
        .await
        .unwrap();
        let cash_out = record_cash_out(
            &pool,
            "20.00".to_string(),
            Some("compra de insumos".to_string()),
            "key-cash-out-history",
        )
        .await
        .unwrap();

        let sale_id = sale.sale_id;
        let movement_id = cash_out.movement_id;
        let sale_id_for_update = sale_id.clone();
        let movement_id_for_update = movement_id.clone();
        pool.get()
            .await
            .unwrap()
            .interact(move |conn| {
                conn.execute(
                    "UPDATE sales SET created_at = '2026-08-03T10:15:00Z' WHERE id = ?1",
                    [&sale_id_for_update],
                )?;
                conn.execute(
                    "UPDATE movements SET created_at = '2026-08-03T10:30:00Z' WHERE id = ?1",
                    [&movement_id_for_update],
                )?;
                Ok::<_, rusqlite::Error>(())
            })
            .await
            .unwrap()
            .unwrap();

        let history = get_session_history(&pool).await.unwrap();

        assert_eq!(history.operations.len(), 2);
        assert_eq!(history.operations[0].id, sale_id);
        assert_eq!(history.operations[0].operation_type, "sale");
        assert_eq!(history.operations[0].amount, "120.00");
        assert_eq!(
            history.operations[0].detail.as_deref(),
            Some("campera y cuaderno")
        );
        assert_eq!(history.operations[0].category.as_deref(), Some("mixto"));
        assert_eq!(history.operations[0].payments.len(), 2);
        let transfer = history.operations[0]
            .payments
            .iter()
            .find(|payment| payment.method == "transfer")
            .unwrap();
        assert_eq!(transfer.recipient_account.as_deref(), Some("Juan"));
        assert_eq!(history.transfer_totals.len(), 1);
        assert_eq!(history.transfer_totals[0].recipient_account, "Juan");
        assert_eq!(history.transfer_totals[0].amount, "50.00");
        assert_eq!(history.operations[1].id, movement_id);
        assert_eq!(history.operations[1].operation_type, "cash_out");
        assert_eq!(
            history.operations[1].detail.as_deref(),
            Some("compra de insumos")
        );
        assert_eq!(history.payment_totals.cash, "70.00");
        assert_eq!(history.payment_totals.transfer, "50.00");
        assert_eq!(history.payment_totals.debit, "0.00");
        assert_eq!(history.expected_cash, "150.00");
        assert_eq!(history.operations[0].occurred_at, "2026-08-03T10:15:00Z");
        assert_eq!(history.operations[1].occurred_at, "2026-08-03T10:30:00Z");
    }

    #[tokio::test]
    async fn session_history_groups_net_transfer_totals_by_account() {
        let (_dir, pool) = setup_db().await;
        open_session(&pool, 10000, "key-open").await.unwrap();

        let split_sale = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "100.00".to_string(),
                category: "mixto".to_string(),
                detail: None,
                payments: vec![
                    PaymentPartRequest {
                        method: "cash".to_string(),
                        amount: "40.00".to_string(),
                        recipient_account: None,
                    },
                    PaymentPartRequest {
                        method: "transfer".to_string(),
                        amount: "30.00".to_string(),
                        recipient_account: Some("Juan".to_string()),
                    },
                    PaymentPartRequest {
                        method: "transfer".to_string(),
                        amount: "30.00".to_string(),
                        recipient_account: Some("Carlos".to_string()),
                    },
                ],
            },
            "key-sale-split-accounts",
        )
        .await
        .unwrap();
        record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "20.00".to_string(),
                category: "ropa".to_string(),
                detail: None,
                payments: vec![PaymentPartRequest {
                    method: "transfer".to_string(),
                    amount: "20.00".to_string(),
                    recipient_account: Some("Carlos".to_string()),
                }],
            },
            "key-sale-carlos",
        )
        .await
        .unwrap();

        let before = get_session_history(&pool).await.unwrap();
        assert_eq!(before.payment_totals.transfer, "80.00");
        assert_eq!(before.transfer_totals.len(), 2);
        assert_eq!(before.transfer_totals[0].recipient_account, "Carlos");
        assert_eq!(before.transfer_totals[0].amount, "50.00");
        assert_eq!(before.transfer_totals[1].recipient_account, "Juan");
        assert_eq!(before.transfer_totals[1].amount, "30.00");
        assert_eq!(before.expected_cash, "140.00");

        crate::application::sale_use_case::annul_sale(
            &pool,
            crate::application::sale_use_case::AnnulSaleRequest {
                sale_id: split_sale.sale_id,
                reason: Some("error de carga".to_string()),
            },
            "key-annul-split-accounts",
        )
        .await
        .unwrap();

        let after = get_session_history(&pool).await.unwrap();
        assert_eq!(after.payment_totals.transfer, "20.00");
        assert_eq!(after.transfer_totals.len(), 1);
        assert_eq!(after.transfer_totals[0].recipient_account, "Carlos");
        assert_eq!(after.transfer_totals[0].amount, "20.00");
        assert_eq!(after.expected_cash, "100.00");
    }

    #[tokio::test]
    async fn closed_session_list_is_empty_then_reverse_chronological() {
        let (_dir, pool) = setup_db().await;

        assert!(list_closed_sessions(&pool).await.unwrap().is_empty());

        pool.get()
            .await
            .unwrap()
            .interact(|conn| {
                conn.execute(
                    "INSERT INTO cash_sessions (id, status, opening_amount, counted_cash, difference, opened_at, closed_at) VALUES (?1, 'closed', ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        "closed-old",
                        1000,
                        1000,
                        0,
                        "2026-08-01T09:00:00Z",
                        "2026-08-01T18:00:00Z"
                    ],
                )?;
                conn.execute(
                    "INSERT INTO cash_sessions (id, status, opening_amount, counted_cash, difference, opened_at, closed_at) VALUES (?1, 'closed', ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![
                        "closed-new",
                        2000,
                        2100,
                        100,
                        "2026-08-02T09:00:00Z",
                        "2026-08-02T18:00:00Z"
                    ],
                )?;
                Ok::<_, rusqlite::Error>(())
            })
            .await
            .unwrap()
            .unwrap();

        let sessions = list_closed_sessions(&pool).await.unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].session_id, "closed-new");
        assert_eq!(sessions[0].opened_at, "2026-08-02T09:00:00Z");
        assert_eq!(sessions[0].closed_at, "2026-08-02T18:00:00Z");
        assert_eq!(sessions[0].opening_amount, "20.00");
        assert_eq!(sessions[0].expected_cash, "20.00");
        assert_eq!(sessions[0].counted_cash, "21.00");
        assert_eq!(sessions[0].difference, "1.00");
        assert_eq!(sessions[1].session_id, "closed-old");
    }

    #[tokio::test]
    async fn closed_session_detail_preserves_annulled_split_sale_and_net_totals() {
        let (_dir, pool) = setup_db().await;
        let open = open_session(&pool, 10000, "key-closed-detail-open")
            .await
            .unwrap();

        let annulled_sale = record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "100.00".to_string(),
                category: "mixto".to_string(),
                detail: Some("campera y cuaderno".to_string()),
                payments: vec![
                    PaymentPartRequest {
                        method: "cash".to_string(),
                        amount: "40.00".to_string(),
                        recipient_account: None,
                    },
                    PaymentPartRequest {
                        method: "transfer".to_string(),
                        amount: "30.00".to_string(),
                        recipient_account: Some("Juan".to_string()),
                    },
                    PaymentPartRequest {
                        method: "transfer".to_string(),
                        amount: "30.00".to_string(),
                        recipient_account: Some("Carlos".to_string()),
                    },
                ],
            },
            "key-closed-detail-annulled-sale",
        )
        .await
        .unwrap();
        record_sale(
            &pool,
            RecordSaleRequest {
                total_amount: "20.00".to_string(),
                category: "ropa".to_string(),
                detail: Some("bufanda".to_string()),
                payments: vec![PaymentPartRequest {
                    method: "transfer".to_string(),
                    amount: "20.00".to_string(),
                    recipient_account: Some("Carlos".to_string()),
                }],
            },
            "key-closed-detail-confirmed-sale",
        )
        .await
        .unwrap();
        let cash_out = record_cash_out(
            &pool,
            "5.00".to_string(),
            Some("compra de insumos".to_string()),
            "key-closed-detail-cash-out",
        )
        .await
        .unwrap();

        crate::application::sale_use_case::annul_sale(
            &pool,
            crate::application::sale_use_case::AnnulSaleRequest {
                sale_id: annulled_sale.sale_id.clone(),
                reason: Some("error de carga".to_string()),
            },
            "key-closed-detail-annulment",
        )
        .await
        .unwrap();

        close_session(&pool, 9500, "key-closed-detail-close")
            .await
            .unwrap();

        let detail = get_closed_session_detail(&pool, &open.session_id)
            .await
            .unwrap();
        assert_eq!(detail.session_id, open.session_id);
        assert_eq!(detail.opening_amount, "100.00");
        assert_eq!(detail.expected_cash, "95.00");
        assert_eq!(detail.counted_cash, "95.00");
        assert_eq!(detail.difference, "0.00");
        assert_eq!(detail.operations.len(), 3);

        let annulled = detail
            .operations
            .iter()
            .find(|operation| operation.id == annulled_sale.sale_id)
            .unwrap();
        assert_eq!(annulled.status.as_deref(), Some("annulled"));
        assert_eq!(annulled.category.as_deref(), Some("mixto"));
        assert_eq!(annulled.detail.as_deref(), Some("campera y cuaderno"));
        assert_eq!(annulled.annulment_reason.as_deref(), Some("error de carga"));
        assert_eq!(annulled.payments.len(), 3);
        assert!(annulled
            .payments
            .iter()
            .any(|payment| payment.recipient_account.as_deref() == Some("Juan")));
        assert!(annulled
            .payments
            .iter()
            .any(|payment| payment.recipient_account.as_deref() == Some("Carlos")));

        let cash_out_operation = detail
            .operations
            .iter()
            .find(|operation| operation.id == cash_out.movement_id)
            .unwrap();
        assert_eq!(cash_out_operation.operation_type, "cash_out");
        assert_eq!(
            cash_out_operation.detail.as_deref(),
            Some("compra de insumos")
        );
        assert_eq!(detail.payment_totals.cash, "0.00");
        assert_eq!(detail.payment_totals.transfer, "20.00");
        assert_eq!(detail.transfer_totals.len(), 1);
        assert_eq!(detail.transfer_totals[0].recipient_account, "Carlos");
        assert_eq!(detail.transfer_totals[0].amount, "20.00");
    }
}
